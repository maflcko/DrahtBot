use super::{Feature, FeatureMeta};
use crate::errors::{DrahtBotError, Result};
use crate::Context;
use crate::GitHubEvent;
use async_trait::async_trait;
use octocrab::models::repos::DiffEntryStatus;
use octocrab::models::AuthorAssociation::{FirstTimeContributor, FirstTimer, Mannequin, None};

pub struct SpamDetectionFeature {
    meta: FeatureMeta,
}

impl SpamDetectionFeature {
    pub fn new() -> Self {
        Self {
            meta: FeatureMeta::new(
                "Spam Detection",
                "Automatically detect and close spam-like pull requests based on simple heuristics.",
                vec![GitHubEvent::PullRequest],
            ),
        }
    }
}

#[async_trait]
impl Feature for SpamDetectionFeature {
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
            GitHubEvent::PullRequest if action == "opened" => {
                // https://docs.github.com/en/developers/webhooks-and-events/webhooks/webhook-events-and-payloads#pull_request
                let pr_number = payload["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                let issues_api = ctx.octocrab.issues(repo_user, repo_name);
                let pulls_api = ctx.octocrab.pulls(repo_user, repo_name);
                spam_detection(
                    &ctx.octocrab,
                    &issues_api,
                    &pulls_api,
                    pr_number,
                    ctx.dry_run,
                )
                .await?;
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
    if all_files.iter().any(|f| {
        let sw = |p| f.filename.starts_with(p);
        let ct = |p| f.filename.contains(p);
        sw("README.md")
            || sw("doc/release-notes/release-notes-")
            || sw("INSTALL.md")
            || ct("CONTRIBUTING")
            || ct("LICENSE")
            || ct(".devcontainer/devcontainer.json")
            || ct("SECURITY")
            || ct("FUNDING")
    }) || all_files.iter().all(|f| f.status == DiffEntryStatus::Added)
        || all_files
            .iter()
            .any(|f| f.filename.starts_with(".github") && f.status == DiffEntryStatus::Added)
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
