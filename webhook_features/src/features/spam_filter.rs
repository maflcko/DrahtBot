use super::{Feature, FeatureMeta};
use crate::errors::DrahtBotError;
use crate::errors::Result;
use crate::Context;
use crate::GitHubEvent;
use async_trait::async_trait;

pub struct SpamFilterFeature {
    meta: FeatureMeta,
}

impl SpamFilterFeature {
    pub fn new() -> Self {
        Self {
            meta: FeatureMeta::new(
                "SpamFilter",
                "Fight spam with heuristics.",
                vec![GitHubEvent::Issues],
            ),
        }
    }
}

#[async_trait]
impl Feature for SpamFilterFeature {
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
            GitHubEvent::Issues if action == "opened" => {
                // https://docs.github.com/en/developers/webhooks-and-events/webhooks/webhook-events-and-payloads#issues
                let issue_body = payload["issue"]["body"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                let issue_number = payload["issue"]["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                let issues_api = ctx.octocrab.issues(repo_user, repo_name);
                if issue_body.contains("_Originally posted by @") {
                    issues_api
                        .update(issue_number)
                        .title(".")
                        .body("Removed likely Spam")
                        .state(octocrab::models::IssueState::Closed)
                        .send()
                        .await?;
                    issues_api
                        .lock(issue_number, octocrab::params::LockReason::Spam)
                        .await?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
