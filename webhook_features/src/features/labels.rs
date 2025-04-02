use super::{Feature, FeatureMeta};
use crate::errors::DrahtBotError;
use crate::errors::Result;
use crate::Context;
use crate::GitHubEvent;
use async_trait::async_trait;
use octocrab::models::AuthorAssociation::{FirstTimeContributor, FirstTimer, Mannequin, None};

pub struct LabelsFeature {
    meta: FeatureMeta,
}

impl LabelsFeature {
    pub fn new() -> Self {
        Self {
            meta: FeatureMeta::new(
                "Labels",
                "Guess and set labels on pull requests missing them.",
                vec![GitHubEvent::PullRequest],
            ),
        }
    }
}

#[async_trait]
impl Feature for LabelsFeature {
    fn meta(&self) -> &FeatureMeta {
        &self.meta
    }

    async fn handle(
        &self,
        ctx: &Context,
        event: &GitHubEvent,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let action = payload["action"]
            .as_str()
            .ok_or(DrahtBotError::KeyNotFound)?;

        let repo_user = payload["repository"]["owner"]["login"]
            .as_str()
            .ok_or(DrahtBotError::KeyNotFound)?;

        let repo_name = payload["repository"]["name"]
            .as_str()
            .ok_or(DrahtBotError::KeyNotFound)?;

        println!("Handling: {repo_user}/{repo_name} {event}::{action}");
        match event {
            GitHubEvent::PullRequest
                if action == "unlabeled" || action == "opened" || action == "edited" =>
            {
                // https://docs.github.com/en/developers/webhooks-and-events/webhooks/webhook-events-and-payloads#pull_request
                if let Some(config_repo) = ctx
                    .config
                    .repositories
                    .iter()
                    .find(|r| r.repo_slug == format!("{repo_user}/{repo_name}"))
                {
                    let pr_number = payload["number"]
                        .as_u64()
                        .ok_or(DrahtBotError::KeyNotFound)?;
                    let base_name = payload["pull_request"]["base"]["repo"]["default_branch"]
                        .as_str()
                        .ok_or(DrahtBotError::KeyNotFound)?;
                    let issues_api = ctx.octocrab.issues(repo_user, repo_name);
                    let pulls_api = ctx.octocrab.pulls(repo_user, repo_name);
                    let pull = pulls_api.get(pr_number).await?;
                    apply_labels_one(
                        &ctx.octocrab,
                        &issues_api,
                        config_repo,
                        base_name,
                        &pull,
                        ctx.dry_run,
                    )
                    .await?;
                }
                if action == "opened" {
                    let pr_number = payload["number"]
                        .as_u64()
                        .ok_or(DrahtBotError::KeyNotFound)?;
                    let pr_title = payload["pull_request"]["title"]
                        .as_str()
                        .ok_or(DrahtBotError::KeyNotFound)?;
                    let issues_api = ctx.octocrab.issues(repo_user, repo_name);
                    let pulls_api = ctx.octocrab.pulls(repo_user, repo_name);
                    spam_detection(
                        &ctx.octocrab,
                        &issues_api,
                        &pulls_api,
                        pr_number,
                        pr_title,
                        ctx.dry_run,
                    )
                    .await?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

async fn spam_detection(
    github: &octocrab::Octocrab,
    issues_api: &octocrab::issues::IssueHandler<'_>,
    pulls_api: &octocrab::pulls::PullRequestHandler<'_>,
    pr_number: u64,
    pr_title: &str,
    dry_run: bool,
) -> Result<()> {
    let all_files = github
        .all_pages(pulls_api.list_files(pr_number).await?)
        .await?;
    if all_files
        .iter()
        .any(|f| f.filename.starts_with("doc/release-notes/release-notes-") && f.deletions > 0)
    {
        let text = "📁 Archived release notes are archived and should not be modified.";
        if !dry_run {
            issues_api.create_comment(pr_number, text).await?;
        }
    }
    if all_files.iter().all(|f| {
        let sw = |p| f.filename.starts_with(p);
        sw("README.md")
            || sw("CONTRIBUTING.md")
            || sw("COPYING")
            || sw(".devcontainer/devcontainer.json")
    }) || pr_title.starts_with("Create ") && all_files.len() == 1
    {
        let pull_request = pulls_api.get(pr_number).await?;
        if [FirstTimer, FirstTimeContributor, Mannequin, None]
            .contains(&pull_request.author_association.unwrap())
        {
            let reason =
                "♻️ Automatically closing for now based on heuristics. Please leave a comment, if this was erroneous. Generally, please focus on creating high-quality, original content that demonstrates a clear understanding of the project's requirements and goals.";
            if !dry_run {
                issues_api.create_comment(pr_number, reason).await?;
                issues_api
                    .update(pr_number)
                    .state(octocrab::models::IssueState::Closed)
                    .send()
                    .await?;
            }
        }
    }
    Ok(())
}

async fn apply_labels_one(
    github: &octocrab::Octocrab,
    issues_api: &octocrab::issues::IssueHandler<'_>,
    config_repo: &crate::config::Repo,
    base_name: &str,
    pull: &octocrab::models::pulls::PullRequest,
    dry_run: bool,
) -> Result<()> {
    let regs = config_repo.repo_labels.iter().fold(
        std::collections::HashMap::<&String, Vec<regex::Regex>>::new(),
        |mut acc, (label_name, title_regs)| {
            for reg in title_regs {
                acc.entry(label_name).or_default().push(
                    regex::RegexBuilder::new(reg)
                        .case_insensitive(true)
                        .build()
                        .expect("regex config format error"),
                );
            }
            acc
        },
    );
    let pull_title = pull.title.as_ref().expect("remote api error");
    let pull_title_trimmed = pull_title.trim();
    if pull_title_trimmed != pull_title && !dry_run {
        issues_api
            .update(pull.number)
            .title(pull_title_trimmed)
            .send()
            .await?;
    }
    let pull_title = pull_title_trimmed;
    let labels = github
        .all_pages(issues_api.list_labels_for_issue(pull.number).send().await?)
        .await?;
    if !labels.is_empty() {
        return Ok(());
    }
    let mut new_labels = Vec::new();
    if pull.base.ref_field != base_name {
        new_labels.push(config_repo.backport_label.to_string());
    } else {
        for (label_name, title_regs) in regs {
            if title_regs.iter().any(|r| r.is_match(pull_title)) {
                new_labels.push(label_name.to_string());
                break;
            }
        }
    }
    if new_labels.is_empty() {
        return Ok(());
    }
    println!(" ... add_to_labels({new_labels:?})");
    if !dry_run {
        issues_api.add_labels(pull.number, &new_labels).await?;
    }
    Ok(())
}
