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
                "Creates a summary comment on pull requests which tracks reviews.",
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

        println!("Handling: {repo_user}/{repo_name} {event}::{action}");
        match event {
            GitHubEvent::PullRequest if action == "synchronize" || action == "opened" => {
                // https://docs.github.com/en/developers/webhooks-and-events/webhooks/webhook-events-and-payloads#pull_request
                let pr_number = payload["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                refresh_summary_comment(ctx, repo, pr_number).await?
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
                    refresh_summary_comment(ctx, repo, pr_number).await?
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
                    refresh_summary_comment(ctx, repo, pr_number).await?
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn summary_comment_template(reviews: Vec<Review>) -> String {
    let mut comment = r#"
### Reviews
See [the guideline](https://github.com/bitcoin/bitcoin/blob/master/CONTRIBUTING.md#code-review) for information on the review process.
If your review is incorrectly listed, please react with 👎 to this comment and the bot will ignore it.
"#
    .to_string();

    if reviews.is_empty() {
        comment += "A summary of reviews will appear here.\n";
    } else {
        comment += "| Type | Reviewers |\n";
        comment += "| ---- | --------- |\n";

        let mut ack_map: HashMap<AckType, Vec<(String, String, chrono::DateTime<chrono::Utc>)>> =
            reviews.into_iter().fold(HashMap::new(), |mut acc, review| {
                acc.entry(review.ack_type).or_default().push((
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
                        .map(|(user, url, _)| format!("[{}]({})", user, url))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }

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

async fn refresh_summary_comment(ctx: &Context, repo: Repository, pr_number: u64) -> Result<()> {
    let pr = ctx
        .octocrab
        .pulls(&repo.owner, &repo.name)
        .get(pr_number)
        .await?;

    let all_comments = ctx
        .octocrab
        .all_pages(
            ctx.octocrab
                .issues(&repo.owner, &repo.name)
                .list_comments(pr_number)
                .send()
                .await?,
        )
        .await?;

    let cmt = util::get_metadata_sections_from_comments(&all_comments, pr_number);

    let ignored_users: Vec<String> = if let Some(cmt_id) = cmt.id {
        let reactions = ctx.octocrab.all_pages(
            ctx.octocrab
                .issues(&repo.owner, &repo.name)
                .list_comment_reactions(cmt_id)
                .send()
                .await?,
        ).await?;

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
            body: c.body_text.unwrap_or_default(),
            date: c.updated_at.unwrap_or(c.created_at),
        })
        .collect::<Vec<_>>();
    let mut all_review_comments = ctx
        .octocrab
        .all_pages(
            ctx.octocrab
                .pulls(&repo.owner, &repo.name)
                .list_reviews(pr_number)
                .await?,
        )
        .await?
        .into_iter()
        .map(|c| GitHubReviewComment {
            user: c.user.login,
            url: c.html_url.to_string(),
            body: c.body_text.unwrap_or_default(),
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
            let has_current_head = ac.commit.map_or(false, |c| head_commit.starts_with(&c));
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

    let parsed_acks = user_reviews
        .into_iter()
        .map(|e| e.1.into_iter().max_by_key(|r| r.date).unwrap())
        .collect::<Vec<_>>();

    let comment = summary_comment_template(parsed_acks);
    util::update_metadata_comment(
        &ctx.octocrab.issues(&repo.owner, &repo.name),
        cmt,
        &comment,
        util::IdComment::SecReviews,
        ctx.dry_run,
    )
    .await?;
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
    Ignored, // The user has a -1 reaction on the summary comment
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
            AckType::Ignored => "Ignored review",
        }
    }
}

lazy_static! {
    static ref ACK_PATTERNS: Vec<(Regex, AckType)> = vec![
        (r"\b(Approach ACK)\b", AckType::ApproachAck),
        (r"\b(Approach NACK)\b", AckType::ApproachNack),
        (r"\b(NACK)\b", AckType::ConceptNack),
        (r"\b(Concept ACK)\b", AckType::ConceptAck),
        (r"(ACK)(?:\s*)([0-9a-f]{6,40})\b", AckType::Ack),
        (r"\b(ACK)\b", AckType::ConceptAck)
    ]
    .into_iter()
    .map(|(reg, typ)| (Regex::new(reg).unwrap(), typ))
    .collect::<Vec::<_>>();
}

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
                expected: None,
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
            }
        ];

        for test_case in test_cases {
            let actual = parse_review(test_case.comment);
            println!("Test case: {}", test_case.comment);
            assert_eq!(actual, test_case.expected);
        }
    }
}
