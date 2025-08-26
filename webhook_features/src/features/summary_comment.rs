use std::collections::HashMap;

use super::{Feature, FeatureMeta};
use crate::errors::DrahtBotError;
use crate::errors::Result;
use crate::Context;
use crate::GitHubEvent;
use async_trait::async_trait;
use lazy_static::lazy_static;
use regex::Regex;

pub struct SummaryCommentFeature {
    meta: FeatureMeta,
}

struct Repository {
    owner: String,
    name: String,
}

impl SummaryCommentFeature {
    pub fn new() -> Self {
        Self {
            meta: FeatureMeta::new(
                "Summary Comment",
                "Creates a summary comment on pull requests which tracks code-review related details.",
                vec![
                    GitHubEvent::IssueComment,
                    GitHubEvent::PullRequest,
                    GitHubEvent::PullRequestReview,
                ],
            ),
        }
    }
}

#[async_trait]
impl Feature for SummaryCommentFeature {
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

        let repo = Repository {
            owner: repo_user.to_string(),
            name: repo_name.to_string(),
        };

        println!(
            "Handling: {repo_user}/{repo_name} {event}::{action} ({feature_name})",
            feature_name = self.meta().name()
        );
        match event {
            GitHubEvent::PullRequest if action == "synchronize" || action == "opened" => {
                // https://docs.github.com/en/developers/webhooks-and-events/webhooks/webhook-events-and-payloads#pull_request
                let pr_number = payload["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                let diff_url = payload["pull_request"]["diff_url"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?
                    .to_string();
                refresh_summary_comment(ctx, repo, pr_number, Some(diff_url)).await?
            }
            GitHubEvent::IssueComment if payload["issue"].get("pull_request").is_some() => {
                // https://docs.github.com/en/developers/webhooks-and-events/webhooks/webhook-events-and-payloads#issue_comment
                let comment_author = payload["comment"]["user"]["login"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                let pr_number = payload["issue"]["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                if payload["issue"]["state"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?
                    == "open"
                    && comment_author != ctx.bot_username
                {
                    refresh_summary_comment(ctx, repo, pr_number, None).await?
                }
            }
            GitHubEvent::PullRequestReview => {
                // https://docs.github.com/en/developers/webhooks-and-events/webhooks/webhook-events-and-payloads#pull_request_review
                let pr_number = payload["pull_request"]["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                if payload["pull_request"]["state"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?
                    == "open"
                {
                    refresh_summary_comment(ctx, repo, pr_number, None).await?
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn summary_comment_template(reviews: Vec<Review>) -> String {
    let review_url = "https://github.com/bitcoin/bitcoin/blob/master/CONTRIBUTING.md#code-review";
    let mut comment = format!(
        r#"
### Reviews
See [the guideline]({review_url}) for information on the review process.
"#
    );
    if reviews.is_empty() {
        comment += "A summary of reviews will appear here.\n";
    } else {
        comment += "| Type | Reviewers |\n";
        comment += "| ---- | --------- |\n";

        let mut ack_map = reviews.into_iter().fold(HashMap::new(), |mut acc, review| {
            acc.entry(review.ack_type).or_insert(Vec::<_>::new()).push((
                review.user,
                review.url,
                review.date,
            ));
            acc
        });

        // Display ACKs in the following order
        for ack_type in &[
            AckType::Ack,
            AckType::ConceptNack,
            AckType::ConceptAck,
            AckType::ApproachAck,
            AckType::ApproachNack,
            AckType::StaleAck,
            AckType::Ignored,
        ] {
            if let Some(mut users) = ack_map.remove(ack_type) {
                // Sort by date
                users.sort_by_key(|u| u.2);
                comment += &format!(
                    "| {} | {} |\n",
                    ack_type.as_str(),
                    users
                        .iter()
                        .map(|(user, url, _)| format!("[{user}]({url})"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }

        comment += "\n";
        comment +="If your review is incorrectly listed, please react with 👎 to this comment and the bot will ignore it on the next update.";
        comment += "\n";
    }

    comment
}

struct GitHubReviewComment {
    user: String,
    url: String,
    body: String,
    date: chrono::DateTime<chrono::Utc>,
}

async fn refresh_summary_comment(
    ctx: &Context,
    repo: Repository,
    pr_number: u64,
    llm_diff_pr: Option<String>,
) -> Result<()> {
    println!("Refresh summary comment for {pr_number}");
    let issues_api = ctx.octocrab.issues(&repo.owner, &repo.name);
    let pulls_api = ctx.octocrab.pulls(&repo.owner, &repo.name);
    let pr = pulls_api.get(pr_number).await?;

    let all_comments = ctx
        .octocrab
        .all_pages(issues_api.list_comments(pr_number).send().await?)
        .await?;

    let mut cmt = util::get_metadata_sections_from_comments(&all_comments, pr_number);

    if let Some(config_repo) = ctx
        .config
        .repositories
        .iter()
        .find(|r| r.repo_slug == format!("{}/{}", repo.owner, repo.name))
    {
        if config_repo.corecheck {
            let coverage = r#"
### Code Coverage & Benchmarks
For details see: https://corecheck.dev/{owner}/{repo}/pulls/{pull_num}.
"#;
            util::update_metadata_comment(
                &issues_api,
                &mut cmt,
                &coverage
                    .replace("{owner}", &repo.owner)
                    .replace("{repo}", &repo.name)
                    .replace("{pull_num}", &pr_number.to_string()),
                util::IdComment::SecCodeCoverage,
                ctx.dry_run,
            )
            .await?;
        }
    }

    if let Some(url) = llm_diff_pr {
        let mut text = "".to_string();
        match get_llm_check(&url, &ctx.llm_token).await {
            Ok(reply) => {
                if reply.contains("No typos were found") {
                    // text remains empty
                } else {
                    let section = r#"
### LLM Linter (✨ experimental)

Possible typos and grammar issues:

{llm_reply}

<sup>drahtbot_id_{d_id}</sup>
"#;
                    text = section
                        .replace("{llm_reply}", &reply)
                        .replace("{d_id}", "5_m");
                }
            }
            Err(err) => {
                println!(" ... ERROR when requesting llm check {:?}", err);
                // text remains empty
            }
        }
        util::update_metadata_comment(
            &issues_api,
            &mut cmt,
            &text,
            util::IdComment::SecLmCheck,
            ctx.dry_run,
        )
        .await?;
    }

    let ignored_users = if let Some(cmt_id) = cmt.id {
        let reactions = ctx
            .octocrab
            .all_pages(issues_api.list_comment_reactions(cmt_id).send().await?)
            .await?;

        reactions
            .into_iter()
            .filter(|r| r.content == octocrab::models::reactions::ReactionContent::MinusOne)
            .map(|r| r.user.login)
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    let mut all_comments = all_comments
        .into_iter()
        .filter(|c| cmt.id != Some(c.id))
        .map(|c| GitHubReviewComment {
            user: c.user.login,
            url: c.html_url.to_string(),
            body: c.body.unwrap_or_default(),
            date: c.updated_at.unwrap_or(c.created_at),
        })
        .collect::<Vec<_>>();
    let mut all_review_comments = ctx
        .octocrab
        .all_pages(pulls_api.list_reviews(pr_number).send().await?)
        .await?
        .into_iter()
        .filter(|c| c.user.is_some())
        .map(|c| GitHubReviewComment {
            user: c.user.unwrap().login,
            url: c.html_url.to_string(),
            body: c.body.unwrap_or_default(),
            date: c.submitted_at.unwrap(),
        })
        .collect::<Vec<_>>();

    all_comments.append(&mut all_review_comments);

    let head_commit = pr.head.sha;

    let mut user_reviews: HashMap<String, Vec<Review>> = HashMap::new(); // Need to store all acks per user to avoid duplicates

    println!(
        " ... Refresh of {num} comments from {url}.",
        num = all_comments.len(),
        url = pr.html_url.unwrap(),
    );

    let pr_author = pr.user.unwrap().login;
    for comment in all_comments.into_iter() {
        if comment.user == pr_author {
            continue;
        }
        if let Some(ac) = parse_review(&comment.body) {
            let v = user_reviews.entry(comment.user.clone()).or_default();
            let has_current_head = ac.commit.is_some_and(|c| head_commit.starts_with(&c));
            v.push(Review {
                user: comment.user.clone(),
                ack_type: if ignored_users.contains(&comment.user) {
                    AckType::Ignored
                } else if ac.ack_type == AckType::Ack && !has_current_head {
                    AckType::StaleAck
                } else {
                    ac.ack_type
                },
                url: comment.url,
                date: comment.date,
            });
        }
    }

    let user_reviews = user_reviews
        .into_iter()
        .map(|e| {
            let e = e.1;
            if let Some(ack) = e.iter().find(|r| r.ack_type == AckType::Ack) {
                // Prefer ACK commit_hash over anything, to match the behavior of
                // https://github.com/bitcoin-core/bitcoin-maintainer-tools/blob/f9b845614f7aecb9423d0621375e1bad17f92fde/github-merge.py#L208
                ack.clone()
            } else {
                // Fallback to the most recent comment, otherwise
                e.into_iter().max_by_key(|r| r.date).unwrap()
            }
        })
        .collect::<Vec<_>>();

    let max_ack_date = user_reviews
        .iter()
        .filter(|r| r.ack_type == AckType::Ack)
        .max_by_key(|r| r.date)
        .map(|r| r.date);

    // Re-request reviewers.
    // Ideally, do this after some time (7 days) after the last push to avoid requesting reviewers
    // on a pull that did not finish CI yet and to avoid too agressive spam.
    // However, the API does not give a last push date, so it would need to be fetched and stored
    // somehow (via a synchronize event, or opened event, or the commit date of the head commit).
    // For now, if there was 1 ACK, assume it happened after sufficient time.
    // This also helps to avoid notification email spam, because the review request is most likely
    // sent out along with the previous ACK comment notification email.
    let stale_reviewers = if let Some(max_ack_date) = max_ack_date {
        user_reviews
            .iter()
            .filter(|r| match r.ack_type {
                // Only mark "weak" reviews as stale when they were done before max_ack_date. This
                // avoids requesting a review when the users just now left a review comment yet to
                // be addressed.
                AckType::ApproachAck => r.date < max_ack_date,
                AckType::ApproachNack => r.date < max_ack_date, // ApproachNack implies ConceptAck
                AckType::ConceptAck => r.date < max_ack_date,
                AckType::StaleAck => true,

                AckType::Ack => false,
                AckType::ConceptNack => false,
                AckType::Ignored => false,
            })
            .map(|r| r.user.clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let maybe_leftover_review_requests = user_reviews
        .iter()
        .filter(|r| r.ack_type == AckType::Ack)
        .map(|r| r.user.clone())
        .collect::<Vec<_>>();

    let comment = summary_comment_template(user_reviews);
    util::update_metadata_comment(
        &issues_api,
        &mut cmt,
        &comment,
        util::IdComment::SecReviews,
        ctx.dry_run,
    )
    .await?;
    if !maybe_leftover_review_requests.is_empty() {
        println!(
            " ... Unrequest review from {:?}",
            maybe_leftover_review_requests
        );
        // Temporarily disabled due to https://support.github.com/ticket/personal/0/2621973
        //pulls_api
        //    .remove_requested_reviewers(pr_number, maybe_leftover_review_requests, [])
        //    .await?;
    }
    // Done one-by-one to work around https://github.com/maflcko/DrahtBot/issues/29
    for stale_reviewer in &stale_reviewers {
        println!(" ... Request review from {}", stale_reviewer);
        if let Err(err) = pulls_api
            .request_reviews(pr_number, [stale_reviewer.to_string()], [])
            .await
        {
            println!(" ... ERROR when requesting review {:?}", err);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum AckType {
    Ack,
    ConceptAck,
    ConceptNack,
    ApproachAck,
    ApproachNack,

    StaleAck, // ACK, but the commit is not the head of the PR anymore
    Ignored,  // The user has a -1 reaction on the summary comment
}

impl AckType {
    fn as_str(&self) -> &str {
        match self {
            AckType::Ack => "ACK",
            AckType::ConceptAck => "Concept ACK",
            AckType::ConceptNack => "Concept NACK",
            AckType::ApproachAck => "Approach ACK",
            AckType::ApproachNack => "Approach NACK",
            AckType::StaleAck => "Stale ACK",
            AckType::Ignored => "User requested bot ignore",
        }
    }
}

lazy_static! {
    static ref ACK_PATTERNS: Vec<(Regex, AckType)> = vec![
        (r"\b(Approach ACK)\b", AckType::ApproachAck),
        (r"\b(Approach NACK)\b", AckType::ApproachNack),
        (r"\b(NACK)\b", AckType::ConceptNack),
        (r"\b(Concept ACK)\b", AckType::ConceptAck),
        (r"(ACK)(?:.*?)([0-9a-f]{6,40})\b", AckType::Ack),
        (r"(ACK)\b", AckType::ConceptAck)
    ]
    .into_iter()
    .map(|(reg, typ)| (Regex::new(reg).unwrap(), typ))
    .collect::<Vec::<_>>();
}

#[derive(Clone)]
struct Review {
    user: String,
    ack_type: AckType,
    url: String,
    date: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, PartialEq)]
struct AckCommit {
    ack_type: AckType,
    commit: Option<String>,
}

fn parse_review(comment: &str) -> Option<AckCommit> {
    let lines = comment.split('\n').filter(|s| !s.starts_with('>'));

    for (re, ack_type) in ACK_PATTERNS.iter() {
        for line in lines.clone() {
            if let Some(caps) = re.captures(line) {
                let commit = caps.get(2).map(|m| m.as_str().to_string());
                return Some(AckCommit {
                    ack_type: *ack_type,
                    commit,
                });
            }
        }
    }
    None
}

async fn get_llm_check(llm_diff_pr: &str, llm_token: &str) -> Result<String> {
    let client = reqwest::Client::new();
    println!(" ... Run LLM check.");
    let diff = client.get(llm_diff_pr).send().await?.text().await?;

    let diff = diff
        .lines()
        .filter(|line| !line.starts_with('-')) // Drop needless lines to avoid confusion and reduce token use
        .map(|line| {
            if line.starts_with('@') {
                "@@ (hunk header) @@" // Rewrite hunk header to avoid typos in hunk header truncated by git
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let payload = serde_json::json!({
      "model": "gpt-5-mini",
      "messages": [
        {
          "role": "developer",
          "content": [
            {
              "type": "text",
              "text":r#"
Identify and provide feedback on typographic or grammatical errors in the provided git diff comments or documentation, focusing exclusively on errors impacting comprehension.

- Only address errors that make the English text invalid or incomprehensible.
- Ignore style preferences, such as the Oxford comma, missing or superfluous commas, awkward but harmless language, and missing or inconsistent punctuation.
- Focus solely on lines added (starting with a + in the diff).
- Address only code comments (for example C++ or Python comments) or documentation (for example markdown).
- If no errors are found, state that no typos were found.

# Output Format

List each error with minimal context, followed by a very brief rationale:
- typo -> replacement [explanation]

If none are found, state: "No typos were found".
"#
    }
          ]
        },
        {
          "role": "user",
          "content": [
            {
              "type": "text",
              "text":diff
              }
          ]
        }
      ],
      "response_format": {
        "type": "text"
      },
      "reasoning_effort": "low",
      "service_tier": "flex",
      "store": true
    });
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", llm_token))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let mut text = response["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(DrahtBotError::KeyNotFound)?
        .to_string();
    if text.is_empty() {
        println!("ERROR: empty llm response: {response}");
        text = "No typos were found".to_string();
    }
    Ok(text)
}

// Test that parse_review works
#[cfg(test)]
mod tests {
    use super::*;

    struct TestCase {
        comment: &'static str,
        expected: Option<AckCommit>,
    }

    #[test]
    fn test_parse_review() {
        let test_cases = vec![
            TestCase {
                comment: "ACK",
                expected: Some(AckCommit {
                    ack_type: AckType::ConceptAck,
                    commit: None,
                }),
            },
            TestCase {
                comment: "ACK 1234567890123456789012345678901234567890",
                expected: Some(AckCommit {
                    ack_type: AckType::Ack,
                    commit: Some("1234567890123456789012345678901234567890".to_string()),
                }),
            },
            TestCase {
                comment: "ACK invalid",
                expected: Some(AckCommit {
                    ack_type: AckType::ConceptAck,
                    commit: None,
                }),
            },
            TestCase {
                comment: "ACK 1234567890123456789012345678901234567890 invalid",
                expected: Some(AckCommit {
                    ack_type: AckType::Ack,
                    commit: Some("1234567890123456789012345678901234567890".to_string()),
                }),
            },
            TestCase {
                comment: "ACK 1234567890123456789012345678901234567890\nACK 1234567890123456789012345678901234567890",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("1234567890123456789012345678901234567890".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "ACK 1234567890123456789012345678901234567890\nNACK 1234567890123456789012345678901234567890",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ConceptNack,
                        commit: None,
                    },
                ),
            },
            TestCase {
                comment: "Concept ACK",
                expected: Some(AckCommit {
                    ack_type: AckType::ConceptAck,
                    commit: None,
                }),
            },
            TestCase {
                comment: "Concept ACK 1234567890123456789012345678901234567890",
                expected: Some(AckCommit {
                    ack_type: AckType::ConceptAck,
                    commit: None,
                }),
            },
            TestCase {
                comment: "tACK",
                expected: Some(AckCommit {
                    ack_type: AckType::ConceptAck,
                    commit: None,
                }),
            },
            TestCase {
                comment: "tACK 1234567890123456789012345678901234567890",
                expected: Some(AckCommit {
                    ack_type: AckType::Ack,
                    commit: Some("1234567890123456789012345678901234567890".to_string()),
                }),
            },
            TestCase {
                comment: "Code Review ACK 123456",
                expected: Some(AckCommit {
                    ack_type: AckType::Ack,
                    commit: Some("123456".to_string()),
                }),
            },
            TestCase {
                comment: "Code Review ACK 1234567890123456789012345678901234567890",
                expected: Some(AckCommit {
                    ack_type: AckType::Ack,
                    commit: Some("1234567890123456789012345678901234567890".to_string()),
                }),
            },
            TestCase {
                comment: "Approach ACK",
                expected: Some(AckCommit {
                    ack_type: AckType::ApproachAck,
                    commit: None,
                }),
            },
            TestCase {
                comment: "Approach ACK 1234567890123456789012345678901234567890",
                expected: Some(AckCommit {
                    ack_type: AckType::ApproachAck,
                    commit: None,
                }),
            },
            TestCase {
                comment: "Concept NACK",
                expected: Some(AckCommit {
                    ack_type: AckType::ConceptNack,
                    commit: None,
                }),
            },
            TestCase {
                comment: "NACK this change!",
                expected: Some(AckCommit {
                    ack_type: AckType::ConceptNack,
                    commit: None,
                }),
            },
            TestCase {
                comment: "> Concept ACK",
                expected: None,
            },
            TestCase {
                comment: "This is a Concept ACK for me!",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ConceptAck,
                        commit: None,
                    },
                ),
            },
            TestCase {
                comment: "This is a Concept ACK for me! 1234567890123456789012345678901234567890",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ConceptAck,
                        commit: None,
                    },
                ),
            },
            TestCase {
                comment: "Code Review ACK",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ConceptAck,
                        commit: None,
                    },
                ),
            },
            TestCase {
                comment: "Code review ACK  bba667e ",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("bba667e".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "Concept ACK, nice.",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ConceptAck,
                        commit: None,
                    },
                ),
            },
            TestCase {
                comment: "ACK    12345678",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("12345678".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "> Good job, ACK 12345678",
                expected: None,
            },
            TestCase {
                comment: "test\nConcept ACK",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ConceptAck,
                        commit: None,
                    }
                )
            },
            TestCase {
                comment: "> NACK \ntest\n\nApproach NACK",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ApproachNack,
                        commit: None,
                    }
                )
            },
            TestCase {
                comment: "> Good job, ACK 12345678\ntest    Concept ACK !",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ConceptAck,
                        commit: None,
                    },
                ),
            },
            TestCase {
                comment: "NACK ACK",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ConceptNack,
                        commit: None,
                    },
                ),
            },
            TestCase {
                comment: "re-ACK 12345678",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("12345678".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "reACK 12345678",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("12345678".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "ReACK 12345678",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("12345678".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "reutACK 12345678",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("12345678".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "CR ACK 12345678",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("12345678".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "crACK 12345678",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("12345678".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "NACK ffaabbccdd11",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::ConceptNack,
                        commit: None,
                    },
                ),
            },
            TestCase {
                comment: "ACK https://github.com/bitcoin/bitcoin/commits/12345678",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("12345678".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "ACK [d9bd628](https://github.com/bitcoin/bitcoin/commit/d9bd628ac9d1e6272cb2f8f67b86376a13233f90)",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("d9bd628".to_string()),
                    },
                ),
            },
            TestCase {
                comment: "ACK https://github.com/bitcoin/bitcoin/pull/12345/commits/12345678",
                expected: Some(
                    AckCommit {
                        ack_type: AckType::Ack,
                        commit: Some("12345678".to_string()),
                    },
                ),
            },
        ];

        for test_case in test_cases {
            let actual = parse_review(test_case.comment);
            println!("Test case: {}", test_case.comment);
            assert_eq!(actual, test_case.expected);
        }
    }
}
