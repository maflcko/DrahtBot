use super::{Feature, FeatureMeta};
use crate::errors::DrahtBotError;
use crate::errors::Result;
use crate::Context;
use crate::GitHubEvent;
use async_trait::async_trait;
use std::process::{Command, Stdio};

pub struct CiStatusFeature {
    meta: FeatureMeta,
}

impl CiStatusFeature {
    pub fn new() -> Self {
        Self {
            meta: FeatureMeta::new(
                "CI Status",
                "Set a label for a failing CI status. Must also be enabled in the config yaml.",
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
        if !ctx
            .config
            .repositories
            .iter()
            .find(|r| r.repo_slug == format!("{}/{}", repo_user, repo_name))
            .is_some_and(|c| c.ci_status)
        {
            return Ok(());
        }
        match event {
            GitHubEvent::CheckSuite if action == "completed" => {
                // https://docs.github.com/webhooks-and-events/webhooks/webhook-events-and-payloads#check_suite
                let conclusion = payload["check_suite"]["conclusion"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                if conclusion == "cancelled" || conclusion == "neutral" {
                    // Fall-through and treat as failure. Will be re-set on the new check_suite
                    // result.
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
                    let mut pull_number = None;
                    // Hacky way to get the pull number. See also https://github.com/maflcko/DrahtBot/issues/59#issuecomment-3472438198
                    for check_run in check_runs.iter().filter(|c| c.output.annotations_count > 0) {
                        let annotations = checks_api
                            .list_annotations(check_run.id)
                            .per_page(99)
                            .send()
                            .await?;
                        for a in annotations.iter().filter(|a| {
                            a.title.as_deref().unwrap_or_default()
                                == "debug_pull_request_number_str"
                        }) {
                            pull_number = Some(
                                a.message
                                    .as_deref()
                                    .ok_or(DrahtBotError::KeyNotFound)?
                                    .parse::<u64>()?,
                            );
                        }
                    }
                    pull_number
                };
                if pull_number.is_none() {
                    return Ok(());
                }
                let pull_number = pull_number.unwrap();
                println!("... pull number {pull_number} conclusion: {conclusion}");
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
                        for run in check_runs
                            .iter()
                            .filter(|r| r.conclusion.as_deref().unwrap_or_default() != "success")
                        {
                            let curl_out = Command::new("curl")
                                .args([
                                    "-L",
                                    "-H",
                                    "Accept: application/vnd.github+json",
                                    "-H",
                                    &format!("Authorization: Bearer {}", ctx.github_token),
                                    "-H",
                                    "X-GitHub-Api-Version: 2022-11-28",
                                    &format!(
                                        "https://api.github.com/repos/{}/{}/actions/jobs/{}/logs",
                                        repo_user, repo_name, run.id
                                    ),
                                ])
                                .stderr(Stdio::inherit())
                                .output()
                                .expect("Failed to execute curl");
                            assert!(curl_out.status.success()); // Could ignore error code or use exit_ok()? in the future.
                            let full_text = String::from_utf8_lossy(&curl_out.stdout);

                            // excerpt
                            let text = full_text
                                .lines()
                                .rev()
                                .take(100) // 100 lines
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect::<Vec<_>>()
                                .join("\n")
                                .chars()
                                .rev()
                                .take(10_000) // 10k unicode chars
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect::<String>();

                            if text.contains("make: *** [Makefile") // build
                                || text.contains("Errors while running CTest")
                                || text.contains("Error: Unexpected dependencies were detected. Check previous output.") // tidy (deps)
                                || text.contains("ailure generated from") // lint, tidy, fuzz
                        {
                            let llm_reason = get_llm_reason(
                                &text,
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
                                check_name = run.name,
                                url = run.html_url.as_deref().unwrap_or_default(),
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
                            break;
                        }
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
