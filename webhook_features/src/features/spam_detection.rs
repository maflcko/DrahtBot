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
                vec![GitHubEvent::PullRequest, GitHubEvent::Issues],
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
        let issues_api = ctx.octocrab.issues(repo_user, repo_name);
        let pulls_api = ctx.octocrab.pulls(repo_user, repo_name);
        match event {
            GitHubEvent::PullRequest => {
                // https://docs.github.com/en/developers/webhooks-and-events/webhooks/webhook-events-and-payloads#pull_request
                let pr_number = payload["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                let title = payload["pull_request"]["title"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                if action == "opened" || action == "edited" {
                    spam_follow_up(&issues_api, title, pr_number, ctx.dry_run).await?;
                }
                if action == "opened" {
                    spam_detection(
                        &ctx.octocrab,
                        &issues_api,
                        &pulls_api,
                        pr_number,
                        ctx.dry_run,
                    )
                    .await?;
                }
                if action == "opened" {
                    let body = payload["pull_request"]["body"]
                        .as_str()
                        .ok_or(DrahtBotError::KeyNotFound)?;
                    spam_llm(
                        &issues_api,
                        title,
                        body,
                        pr_number,
                        &ctx.llm_token,
                        ctx.dry_run,
                    )
                    .await?;
                }
            }
            GitHubEvent::Issues => {
                // https://docs.github.com/en/webhooks/webhook-events-and-payloads?actionType=edited#issues
                let issue_number = payload["issue"]["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                let title = payload["issue"]["title"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                if action == "opened" || action == "edited" {
                    spam_follow_up(&issues_api, title, issue_number, ctx.dry_run).await?;
                }
                if action == "opened" {
                    let body = payload["issue"]["body"]
                        .as_str()
                        .ok_or(DrahtBotError::KeyNotFound)?;
                    spam_llm(
                        &issues_api,
                        title,
                        body,
                        issue_number,
                        &ctx.llm_token,
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

async fn spam_llm(
    issues_api: &octocrab::issues::IssueHandler<'_>,
    title: &str,
    body: &str,
    issue_number: u64,
    llm_token: &str,
    dry_run: bool,
) -> Result<()> {
    let llm_res = get_llm_result(title, body, llm_token)
        .await
        .unwrap_or("NORMAL".to_string());
    if llm_res.starts_with("SPAM") {
        println!(
            "{} detected as likely spam with title={title}",
            issue_number
        );
        let issue = issues_api.get(issue_number).await?;
        if [FirstTimer, FirstTimeContributor, Mannequin, None]
            .contains(&issue.author_association.unwrap())
        {
            let reason = format!(
                r#"
LLM spam detection (✨ experimental): {llm_res}

♻️ Automatically suggested to close for now based on heuristics. Please leave a comment, if this was erroneous.
Generally, please focus on creating high-quality, original content that demonstrates a clear
understanding of the project's requirements and goals.

📝 Moderators: If this is spam, please replace the title with `.`, so that the issue does not appear in
search results.
"#
            );
            if !dry_run {
                issues_api.create_comment(issue_number, reason).await?;
                // Commented out as experiment for now.
                //issues_api
                //    .update(issue_number)
                //    .body(".")
                //    .state(octocrab::models::IssueState::Closed)
                //    .send()
                //    .await?;

                // probably do not want to lock right away? Maybe after 24 hours?
                //issues_api
                //    .lock(issue_number, octocrab::params::LockReason::Spam)
                //    .await?;
            }
        }
    }
    Ok(())
}

async fn get_llm_result(title: &str, body: &str, llm_token: &str) -> Result<String> {
    let client = reqwest::Client::new();
    println!(" ... Run LLM check for spam detection.");
    let question = format!(
        r#"
title: {title}
body: {body}
"#
    );
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
Detect whether the provided GitHub issue or pull request is SPAM or NORMAL based on its content.
Start your reply with either SPAM or NORMAL. If you claim SPAM, you must include an explanation.

- Analyze the title and body to determine if it is legitimate discussion or unwanted/irrelevant promotional content.

**Output Format:**
Start with either:
SPAM
or
NORMAL

**Examples:**

Example 1
Input:
title: Bug: Crash on startup
body: My bitcoin node crashes every time I launch version 0.23. Please advise.

Output:
NORMAL

Example 2
Input:
title: AE917B4 COIN
body: Please describe the feature you'd like to see added. AE917B4.COIN.LOGO.png (view on web)

Output:
SPAM. This issue references a cryptocurrency called "AE917B4 COIN," which is unrelated to Bitcoin. It appears to be a placeholder or generic template with no real context or content, lacking any meaningful description, problem statement, or feature request related to the Bitcoin project.

**Objective Reminder:** Classify GitHub issues and pull requests as either SPAM or NORMAL.
"#
    }
          ]
        },
        {
          "role": "user",
          "content": [
            {
              "type": "text",
              "text": question
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

async fn spam_follow_up(
    issues_api: &octocrab::issues::IssueHandler<'_>,
    title: &str,
    issue_number: u64,
    dry_run: bool,
) -> Result<()> {
    if title.trim() == "." {
        println!(
            "{} detected as spam to close with title={title}",
            issue_number
        );
        if !dry_run {
            issues_api
                .update(issue_number)
                .body(".")
                .state(octocrab::models::IssueState::Closed)
                .send()
                .await?;
            issues_api
                .lock(issue_number, octocrab::params::LockReason::Spam)
                .await?;
        }
    }
    Ok(())
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
            || sw("doc/release-notes/") // Must include trailing slash
            || sw("INSTALL.md")
            || ct("CONTRIBUTING")
            || ct("LICENSE")
            || ct(".devcontainer/devcontainer.json")
            || ct("SECURITY")
            || ct("FUNDING")
    })
        // The next check will also detect a fully empty diff
        || all_files.iter().all(|f| f.status == DiffEntryStatus::Removed)
        || all_files.iter().all(|f| f.status == DiffEntryStatus::Added)
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
