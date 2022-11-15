use std::collections::HashMap;

use super::{Feature, FeatureMeta};
use crate::errors::DrahtBotError;
use crate::errors::Result;
use crate::Context;
use crate::GitHubEvent;
use async_trait::async_trait;
use lazy_static::lazy_static;

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

        println!("Handling event: {:?}", event);
        println!("Action: {action}");
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
                let pr_number = payload["issue"]["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                refresh_summary_comment(ctx, repo, pr_number).await?
            }
            GitHubEvent::PullRequestReview => {
                // https://docs.github.com/en/developers/webhooks-and-events/webhooks/webhook-events-and-payloads#pull_request_review
                let pr_number = payload["pull_request"]["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                refresh_summary_comment(ctx, repo, pr_number).await?
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
"#
    .to_string();

    if reviews.is_empty() {
        comment += "A summary of reviews will appear here.\n";
    } else {
        comment += "| Type | Count | Reviewers |\n";
        comment += "| ---- | ----- | --------- |\n";

        let mut ack_map: HashMap<AckType, Vec<(String, String)>> =
            reviews.into_iter().fold(HashMap::new(), |mut acc, review| {
                acc.entry(review.ack_type)
                    .or_default()
                    .push((review.user, review.url));
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
        ] {
            if let Some(mut users) = ack_map.remove(ack_type) {
                users.sort();
                comment += &format!(
                    "| {} | {} | {} |\n",
                    ack_type.as_str(),
                    users.len(),
                    users
                        .iter()
                        .map(|(user, url)| format!("[{}]({})", user, url))
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
            body: c.body.unwrap_or_default(),
            date: c.submitted_at.unwrap(),
        })
        .collect::<Vec<_>>();

    all_comments.append(&mut all_review_comments);

    let head_commit = pr.head.sha;

    let mut user_reviews: HashMap<String, Vec<Review>> = HashMap::new(); // Need to store all acks per user to avoid duplicates

    println!("Comments count {}", all_comments.len());
    for comment in all_comments.into_iter() {
        if let Some(ac) = parse_review(&comment.body) {
            let v = user_reviews.entry(comment.user.clone()).or_default();
            let has_current_head = ac.commit.map_or(false, |c| head_commit.starts_with(&c));
            v.push(Review {
                user: comment.user,
                ack_type: if ac.ack_type == AckType::Ack && !has_current_head {
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
        false,
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
}

impl AckType {
    fn requires_commit_hash(&self) -> bool {
        matches!(self, AckType::Ack)
    }

    fn as_str(&self) -> &str {
        match self {
            AckType::Ack => "ACK",
            AckType::ConceptAck => "Concept ACK",
            AckType::ConceptNack => "Concept NACK",
            AckType::ApproachAck => "Approach ACK",
            AckType::ApproachNack => "Approach NACK",
            AckType::StaleAck => "Stale ACK",
        }
    }
}

macro_rules! multi_vec {
    ($([$($key:literal),+] => $value:expr);*) => {
        vec![
            $($(($key, $value)),*),*
        ]
    };
}

lazy_static! {
    static ref ACK_PATTERNS: Vec<(&'static str, AckType)> = multi_vec![
        ["code review ack", "cr ack", "cr-ack", "crack"] => AckType::Ack;
        ["concept ack", "concept-ack", "conceptack"] => AckType::ConceptAck;
        ["concept nack", "concept-nack", "conceptnack"] => AckType::ConceptNack;
        ["approach ack", "approach-ack", "approachack"] => AckType::ApproachAck;
        ["approach nack", "approach-nack", "approachnack"] => AckType::ApproachNack;
        ["ack", "utack", "tack"] => AckType::Ack;
        ["nack"] => AckType::ConceptNack
    ];
}

struct Review {
    user: String,
    ack_type: AckType,
    url: String,
    date: chrono::DateTime<chrono::Utc>,
}

fn is_commit_hash(s: &str) -> bool {
    // Use length from https://github.com/bitcoin-core/bitcoin-maintainer-tools/blob/78ab16ae88af7a5ef886ae8cef6df2e9ef3f6085/github-merge.py#L211
    s.len() >= 6 && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, PartialEq)]
struct AckCommit {
    ack_type: AckType,
    commit: Option<String>,
}

fn parse_review(comment: &str) -> Option<AckCommit> {
    let comment = comment.to_lowercase();
    let words = comment
        .split('\n')
        .filter(|s| !s.starts_with('>')) // Ignore quoted text
        .flat_map(|s| s.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())) // Split on whitespace and punctuation
        .collect::<Vec<_>>(); // Collect into a Vec

    // Split words by whitespace and punctuation

    let mut pos = 0;
    while pos < words.len() {
        for (pattern, ack_type) in ACK_PATTERNS.iter() {
            let pattern_words = pattern.split_whitespace().collect::<Vec<_>>(); // Split pattern into words (e.g "code review ack" => ["code", "review", "ack"])

            let pattern_len = {
                match ack_type.requires_commit_hash() {
                    true => pattern_words.len() + 1, // If the ack type requires a commit hash, the pattern will be one word longer
                    false => pattern_words.len(),
                }
            };
            if pattern_len > words.len() - pos {
                // If the pattern is longer than the remaining words, skip it
                continue;
            }

            let mut matches = true;
            for (i, pattern_word) in pattern_words.iter().enumerate() {
                // Check if the pattern matches the words

                // Ignore "re" prefixes, e.g. "reack" => "ack"
                let mut word = words[pos + i].trim_start_matches("re-");
                if word != "review" {
                    word = word.trim_start_matches("re");
                }

                if pattern_word != &word {
                    matches = false;
                    break;
                }
            }

            if matches {
                let mut commit = None;
                if pos + pattern_words.len() < words.len() {
                    // If there are more words after the pattern, check if the next word is a commit hash
                    let next_word = words[pos + pattern_words.len()];
                    if is_commit_hash(next_word) {
                        commit = Some(next_word.to_string()); // If there is a commit hash, attach it to the ack
                    }

                    if ack_type.requires_commit_hash() && commit.is_none() {
                        // If the ack type requires a commit hash, but there is no commit hash, skip this pattern
                        continue;
                    }
                }

                return Some(AckCommit {
                    ack_type: *ack_type,
                    commit,
                });
            }

            if matches {
                pos += pattern_words.len(); // Skip the words that were matched and try to match the next pattern
                break;
            }
        }

        pos += 1;
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
                expected: None,
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
                expected: None,
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
                        ack_type: AckType::Ack,
                        commit: Some("1234567890123456789012345678901234567890".to_string()),
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
                    commit: Some("1234567890123456789012345678901234567890".to_string()),
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
                comment: "crACK",
                expected: None,
            },
            TestCase {
                comment: "crACK 1234567890123456789012345678901234567890",
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
                    commit: Some("1234567890123456789012345678901234567890".to_string()),
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
                comment: "nack this change!",
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
            }
        ];

        for test_case in test_cases {
            let actual = parse_review(test_case.comment);
            println!("Test case: {}", test_case.comment);
            assert_eq!(actual, test_case.expected);
        }
    }
}
