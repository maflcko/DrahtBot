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
                let conclusion = payload["check_suite"]["conclusion"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                if conclusion == "cancelled" || conclusion == "neutral" {
                    // Return early and wait for a new check_suite result
                    return Ok(());
                }
                let success = "success" == conclusion;
                let suite_id = payload["check_suite"]["id"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                let checks_api = ctx.octocrab.checks(repo_user, repo_name);
                let check_runs = checks_api
                    .list_check_runs_in_a_check_suite(suite_id.into())
                    .per_page(99)
                    .send()
                    .await?
                    .check_runs;
                let pull_number = {
                    // Hacky way to get the pull number. See also https://github.com/bitcoin/bitcoin/issues/27178#issuecomment-1503475232
                    let cirrus_task_id = check_runs
                        .first()
                        .ok_or(DrahtBotError::KeyNotFound)?
                        .details_url
                        .as_ref()
                        .ok_or(DrahtBotError::KeyNotFound)?
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
                        .await?;

                    response.json::<serde_json::Value>().await?["data"]["task"]["build"]
                        ["pullRequest"]
                        .as_u64()
                };
                if pull_number.is_none() {
                    return Ok(());
                }
                let pull_number = pull_number.unwrap();
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
                    println!(
                        "... {} add label '{}' due to {}",
                        pull_number, ci_failed_label, conclusion
                    );
                    if !ctx.dry_run {
                        issues_api
                            .add_labels(pull_number, &[ci_failed_label.to_string()])
                            .await?;
                        // Check if *compile* failed and add comment
                        // (functional tests are ignored due to intermittent issues)
                        if let Some(first_fail) = check_runs.iter().find(|r| {
                            let text = r.output.text.clone().unwrap_or_default();
                            text.contains("make: *** [Makefile") // build
                                || text.contains("Errors while running CTest")
                                || text.contains("clang-tidy-")
                                || text.contains("ailure generated from") // lint
                                || text.contains("Test unit written to ") // fuzz
                        }) {
                            let comment = format!(
                                "{}\n{}\n<sub>Debug: {}</sub>\n{}",
                                util::IdComment::CiFailed.str(),
                                "🚧 At least one of the CI tasks failed.",
                                first_fail.html_url.clone().unwrap_or_default(),
                                r#"
<details><summary>Hints</summary>

Make sure to run all tests locally, according to the documentation.

The failure may happen due to a number of reasons, for example:

* Possibly due to a silent merge conflict (the changes in this pull request being
incompatible with the current code in the target branch). If so, make sure to rebase on the latest
commit of the target branch.

* A sanitizer issue, which can only be found by compiling with the sanitizer and running the
  affected test.

* An intermittent issue.

Leave a comment here, if you need help tracking down a confusing failure.

</details>
"#,
                            );
                            issues_api.create_comment(pull_number, comment).await?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
