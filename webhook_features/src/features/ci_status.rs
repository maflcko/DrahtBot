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
                // https://docs.github.com/en/webhooks/webhook-events-and-payloads?actionType=completed#check_suite
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
                        if let Some(pr_str) = annotations.iter().find(|a| {
                            a.title.as_deref().unwrap_or_default()
                                == "debug_pull_request_number_str"
                        }) {
                            pull_number = Some(
                                pr_str
                                    .message
                                    .as_deref()
                                    .ok_or(DrahtBotError::KeyNotFound)?
                                    .parse::<u64>()?,
                            );
                            break;
                        }
                    }
                    pull_number
                };
                if pull_number.is_none() {
                    return Ok(());
                }
                let pull_number = pull_number.unwrap();
                println!("... pull number {pull_number} conclusion: {conclusion}");
                let pull = ctx
                    .octocrab
                    .pulls(repo_user, repo_name)
                    .get(pull_number)
                    .await?;
                if pull.state != Some(octocrab::models::IssueState::Open) {
                    return Ok(());
                };
                // Skip check suites for commits that are no longer the head of the pull request.
                let suite_sha = payload["check_suite"]["head_sha"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                if pull.head.sha != suite_sha {
                    println!(
                        "... skip stale check_suite for {suite_sha} (pull head is {})",
                        pull.head.sha
                    );
                    return Ok(());
                }
                let issues_api = ctx.octocrab.issues(repo_user, repo_name);
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
                        maybe_add_ci_failure_comment(
                            ctx,
                            repo_user,
                            repo_name,
                            pull_number,
                            &check_runs,
                        )
                        .await?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

async fn maybe_add_ci_failure_comment(
    ctx: &Context,
    repo_user: &str,
    repo_name: &str,
    pull_number: u64,
    check_runs: &[octocrab::models::checks::CheckRun],
) -> Result<()> {
    let issues_api = ctx.octocrab.issues(repo_user, repo_name);

    // Only inspect failed runs; cancelled and skipped runs may have no useful log.
    for run in check_runs
        .iter()
        .filter(|r| r.conclusion.as_deref() == Some("failure"))
    {
        let curl_out = Command::new("curl")
            .args([
                "--fail",
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
        if full_text.trim().is_empty() {
            continue;
        }

        // excerpt
        let text = full_text
            .lines()
            .rev()
            .take(500) // lines
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
            .chars()
            .rev()
            .take(20_000) // unicode chars
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>();

        let llm_reason = get_llm_reason(&text, &run.name, &ctx.llm_token)
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

* Intermittent network issues can occur; re-run CI for now, and report the issue if they keep happening.

* A functional or integration test failure may be intermittent or unrelated to this pull request.
  If no dedicated issue thread exists, create a new one to track it.

Leave a comment here, if you need help tracking down a confusing failure.

</details>
"#,
        );
        issues_api.create_comment(pull_number, comment).await?;
        break;
    }
    Ok(())
}

async fn get_llm_reason(ci_log: &str, check_name: &str, llm_token: &str) -> Result<String> {
    let client = reqwest::Client::new();
    println!(" ... Run LLM summary for CI failure.");
    let payload = serde_json::json!({
      "model": "gpt-6-luna",
      "messages": [
        {
          "role": "developer",
          "content": [
            {
              "type": "text",
              "text": format!(r#"
Analyze the provided CI log tail, even if it is truncated, and determine the underlying reason for the failure.

The check run name is `{check_name}`. Use it only as context, and base the classification on evidence in the log.

Classify the primary cause using exactly one of these categories:

- intermittent_network: A likely transient CI network or external service connectivity failure. Use only when the log supports an infrastructure or external-service cause; a network-dependent test failing by itself is not enough.
- build_fail: A compiler, linker, or other build failure.
- test_fail: A unit or fuzz test failure, excluding functional tests.
- functional_test_fail: A functional or integration test failure.
- lint_fail: A failure in a lint check.
- tidy_fail: A clang-tidy or dependency-tidy check failure.
- other_fail: Another identifiable CI failure that does not fit the categories above.
- unknown_ci_fail: The log tail does not provide enough evidence to identify the cause.

If multiple errors appear, choose the most significant underlying cause. Do not infer a cause that is not supported by the log.

# Output Format

A single line in the format `category: explanation`, using one category listed above.
"#)
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
      "reasoning_effort": "medium",
      "service_tier": "default",
      "store": true
    });
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", llm_token))
        .json(&payload)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    let text = response["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(DrahtBotError::KeyNotFound)?
        .to_string();
    Ok(text)
}
