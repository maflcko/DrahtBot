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

        println!(
            "Handling: {repo_user}/{repo_name} {event}::{action} ({feature_name})",
            feature_name = self.meta().name()
        );
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
                        .next_back()
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
                            let text = r.output.text.as_deref().unwrap_or_default();
                            text.contains("make: *** [Makefile") // build
                                || text.contains("Errors while running CTest")
                                || text.contains("Error: Unexpected dependencies were detected. Check previous output.") // tidy (deps)
                                || text.contains("ailure generated from") // lint, tidy, fuzz
                        }) {
                            let llm_reason = get_llm_reason(
                                first_fail.output.text.as_deref().unwrap_or_default(),
                                &ctx.llm_token,
                            )
                            .await
                            .unwrap_or("(empty)".to_string());
                            let comment = format!(
                                r#"{id}
{msg}
<sub>Task `{check_name}`: {url}</sub>
<sub>LLM reason (✨ experimental): {llm_reason}</sub>
{hints}
"#,
                                id = util::IdComment::CiFailed.str(),
                                msg = "🚧 At least one of the CI tasks failed.",
                                check_name = first_fail.name,
                                url = first_fail.html_url.as_deref().unwrap_or_default(),
                                hints = r#"
<details><summary>Hints</summary>

Try to run the tests locally, according to the documentation. However, a CI failure may still
happen due to a number of reasons, for example:

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

async fn get_llm_reason(ci_log: &str, llm_token: &str) -> Result<String> {
    let client = reqwest::Client::new();
    println!(" ... Run LLM summary for CI failure.");
    let payload = serde_json::json!({
      "model": "gpt-5-nano",
      "messages": [
        {
          "role": "developer",
          "content": [
            {
              "type": "text",
              "text":
r#"
Analyze the tail of a CI log to determine and communicate the underlying reason for the CI failure.

Consider potential causes such as build errors, ctest errors, clang-tidy errors, lint test errors, or fuzz test errors, even if the log is truncated.

# Steps

- Read and parse the provided CI log tail.
- If multiple errors appear, prioritize according to potential severity or probable cause of failure and identify the most significant underlying reason.
- Formulate a concise, one-line summary that clearly communicates the identified reason for the failure in the shortest form possible.

# Output Format

A single short sentence summarizing the underlying reason for the CI failure.
"#
    }
          ]
        },
        {
          "role": "user",
          "content": [
            {
              "type": "text",
              "text":ci_log
              }
          ]
        }
      ],
      "response_format": {
        "type": "text"
      },
      "verbosity": "low",
      "reasoning_effort": "low",
      "service_tier": "flex",
      "store": true
    });
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", llm_token))
        .json(&payload)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let text = response["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(DrahtBotError::KeyNotFound)?
        .to_string();
    Ok(text)
}
