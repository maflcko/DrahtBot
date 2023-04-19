use super::{Feature, FeatureMeta};
use crate::errors::DrahtBotError;
use crate::errors::Result;
use crate::Context;
use crate::GitHubEvent;
use async_trait::async_trait;

pub struct CiStatusFeature {
    meta: FeatureMeta,
}

impl CiStatusFeature {
    pub fn new() -> Self {
        Self {
            meta: FeatureMeta::new(
                "CI Status",
                "Set a label for a failing CI status.",
                vec![GitHubEvent::CheckSuite],
            ),
        }
    }
}

#[async_trait]
impl Feature for CiStatusFeature {
    fn meta(&self) -> &FeatureMeta {
        &self.meta
    }

    async fn handle(
        &self,
        ctx: &Context,
        event: &GitHubEvent,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let ci_failed_label = "CI failed";
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
            GitHubEvent::CheckSuite if action == "completed" => {
                // https://docs.github.com/webhooks-and-events/webhooks/webhook-events-and-payloads#check_suite
                let success = "success"
                    == payload["check_suite"]["conclusion"]
                        .as_str()
                        .ok_or(DrahtBotError::KeyNotFound)?;
                let pull_number = {
                    // Hacky way to get the pull number. See also https://github.com/bitcoin/bitcoin/issues/27178#issuecomment-1503475232
                    let suite_id = payload["check_suite"]["id"]
                        .as_u64()
                        .ok_or(DrahtBotError::KeyNotFound)?;
                    let checks_api = ctx.octocrab.checks(repo_user, repo_name);
                    let cirrus_task_id = checks_api
                        .list_check_runs_in_a_check_suite(suite_id.into())
                        .per_page(1)
                        .send()
                        .await?
                        .check_runs
                        .first()
                        .ok_or(DrahtBotError::KeyNotFound)?
                        .details_url
                        .split('/')
                        .last()
                        .ok_or(DrahtBotError::KeyNotFound)?
                        .to_string();

                    let query = format!(
                        r#"{{ "query": "query GetTaskDetailsById($taskId: ID!) {{ task(id: $taskId) {{ id build {{ id repository {{ owner name }} pullRequest }} }} }}", "variables": {{ "taskId": "{}" }} }}"#,
                        cirrus_task_id
                    );

                    let response = reqwest::Client::new()
                        .post("https://api.cirrus-ci.com/graphql")
                        .header("Content-Type", "application/json")
                        .body(query)
                        .send()
                        .await
                        .map_err(|_| DrahtBotError::KeyNotFound)?;

                    response
                        .json::<serde_json::Value>()
                        .await
                        .map_err(|_| DrahtBotError::KeyNotFound)?["data"]["task"]["build"]
                        ["pullRequest"]
                        .as_u64()
                        .ok_or(DrahtBotError::KeyNotFound)?
                };
                let issues_api = ctx.octocrab.issues(repo_user, repo_name);
                let issue = issues_api.get(pull_number).await?;
                if issue.state != octocrab::models::IssueState::Open {
                    return Ok(());
                };
                let labels = ctx
                    .octocrab
                    .all_pages(issues_api.list_labels_for_issue(pull_number).send().await?)
                    .await?;
                let found_label = labels.into_iter().any(|l| l.name == ci_failed_label);
                if found_label && success {
                    println!("... {} remove label '{}')", pull_number, ci_failed_label);
                    if !ctx.dry_run {
                        issues_api
                            .remove_label(pull_number, &ci_failed_label)
                            .await?;
                    }
                } else if !found_label && !success {
                    println!("... {} add label '{}'", pull_number, ci_failed_label);
                    if !ctx.dry_run {
                        issues_api
                            .add_labels(pull_number, &[ci_failed_label.to_string()])
                            .await?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
