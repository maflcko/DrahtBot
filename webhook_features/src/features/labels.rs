use super::{Feature, FeatureMeta};
use crate::errors::DrahtBotError;
use crate::errors::Result;
use crate::Context;
use crate::GitHubEvent;
use async_trait::async_trait;

pub struct LabelsFeature {
    meta: FeatureMeta,
}

impl LabelsFeature {
    pub fn new() -> Self {
        Self {
            meta: FeatureMeta::new(
                "Labels",
                "Guess and set labels on pull requests missing them, if they are set in the config yaml.",
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

        println!(
            "Handling: {repo_user}/{repo_name} {event}::{action} ({feature_name})",
            feature_name = self.meta().name()
        );
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
            }
            _ => {}
        }
        Ok(())
    }
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
        if let Some(bl) = &config_repo.backport_label {
            new_labels.push(bl.to_string());
        }
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
