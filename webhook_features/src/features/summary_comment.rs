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
                "Creates a summary comment on pulls requests with an ACK tracker.",
                vec![
                    GitHubEvent::PullRequest,
                    GitHubEvent::PullRequestReview,
                    GitHubEvent::IssueComment,
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
        event: GitHubEvent,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let action = payload["action"]
            .as_str()
            .ok_or(DrahtBotError::UnknownEvent)?;

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
        println!("Action: {:?}", payload["action"]);
        match event {
            GitHubEvent::PullRequest if action == "opened" => {
                create_summary_comment(payload, &ctx.octocrab).await?
            }

            GitHubEvent::PullRequest if action == "synchronize" => {
                let pr_number = payload["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;

                refresh_summary_comment(ctx, repo, pr_number).await?
            }
            GitHubEvent::IssueComment if payload["issue"].get("pull_request").is_some() => {
                let comment_author = payload["comment"]["user"]["login"]
                    .as_str()
                    .ok_or(DrahtBotError::KeyNotFound)?;

                if !ctx.debug && comment_author == ctx.bot_username {
                    return Ok(());
                }

                let pr_number = payload["issue"]["number"]
                    .as_u64()
                    .ok_or(DrahtBotError::KeyNotFound)?;
                refresh_summary_comment(ctx, repo, pr_number).await?
            }
            GitHubEvent::PullRequestReview => {
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

fn summary_comment_template(initial: bool, acks: Option<Vec<Review>>) -> String {
    let mut comment = r#"
### Reviews
Please ACK this PR if you have reviewed it and believe it to be ready for merging.
See https://github.com/bitcoin/bitcoin/blob/master/CONTRIBUTING.md#code-review for more information.
"#.to_string();

    if initial || acks.is_none() || acks.as_ref().unwrap().is_empty() {
        comment += "ACKs will appear here.\n";
    } else {
        comment += "| ACK | Count | Reviewers |\n";
        comment += "| --- | ----- | --------- |\n";

        match acks {
            Some(acks) => {
                let mut stale_acks: HashMap<String, String> = HashMap::new();
                let ack_map: HashMap<AckType, Vec<(String, String)>> =
                    acks.iter().rev().fold(HashMap::new(), |mut acc, ack| {
                        if ack.commit.is_some() && !ack.commit.as_ref().unwrap().1 {
                            // Commit is referenced but is stale
                            if ack.ack_type == AckType::ACK // Only add Stale for ACKs
                                && !acks.iter().any(|a| {
                                    a.commit.is_some()
                                        && a.commit.as_ref().unwrap().1
                                        && a.user == ack.user // There is no non-stale ACK from the same user
                                })
                            {
                                if stale_acks.contains_key(&ack.user) {
                                    return acc; // Skip stale ACKs from the same users
                                } else {
                                    stale_acks.insert(ack.user.clone(), ack.url.clone());
                                    // Add stale ACK to the list
                                }

                                acc.entry(AckType::StaleACK)
                                    .or_insert_with(Vec::new)
                                    .push((ack.user.clone(), ack.url.clone())); // Store the user and the URL of the stale ACK
                            }
                            return acc;
                        }
                        if ack.ack_type.requires_commit_hash() && ack.commit.is_none() {
                            // ACK requires a commit hash but none is referenced
                            return acc;
                        }

                        acc.entry(ack.ack_type)
                            .or_insert_with(Vec::new)
                            .push((ack.user.clone(), ack.url.clone())); // Store the user and the URL of the ACK
                        acc
                    });

                // Display ACKs in the following order
                for ack_type in vec![
                    AckType::ACK,
                    AckType::ConceptNACK,
                    AckType::ConceptACK,
                    AckType::ApproachACK,
                    AckType::ApproachNACK,
                    AckType::StaleACK,
                ] {
                    if let Some(users) = ack_map.get(&ack_type) {
                        let mut users = users.clone();
                        users.sort();
                        comment += &format!(
                            "| {} | {} | {} |\n",
                            ack_type.to_string(),
                            users.len(),
                            users
                                .iter()
                                .map(|(user, url)| format!("[{}]({})", user, url))
                                .collect::<Vec<String>>()
                                .join(", ")
                        );
                    }
                }
            }
            None => {}
        }

        comment += "\n";
    }

    comment
}

async fn create_summary_comment(
    payload: &serde_json::Value,
    octocrab: &octocrab::Octocrab,
) -> Result<()> {
    let owner = {
        let login = &payload["pull_request"]["user"]["login"];
        login
            .as_str()
            .ok_or(DrahtBotError::InvalidLogin(login.to_string()))?
    };

    let repo_name = {
        let name = &payload["repository"]["name"];
        name.as_str()
            .ok_or(DrahtBotError::InvalidRepositoryName(name.to_string()))?
    };

    let pr_number = {
        let pr_number = &payload["number"];
        pr_number
            .as_u64()
            .ok_or(DrahtBotError::InvalidPullRequestNumber(
                pr_number.to_string(),
            ))?
    };

    let cmt = util::get_metadata_sections(&octocrab, &octocrab.issues(owner, repo_name), pr_number).await?;
    let comment = summary_comment_template(true, None);

    util::update_metadata_comment(
        &octocrab.issues(owner.clone(), repo_name.clone()),
        cmt,
        &comment,
        util::IdComment::SecReviews,
        false).await?;

    Ok(())
}

fn should_skip_ack(commit_ack: AckCommit, commit_acks: Vec<AckCommit>) -> bool {
    match commit_ack.ack_type.requires_commit_hash() {
        true => commit_acks
            .iter()
            .any(|c| c.ack_type == commit_ack.ack_type && c.commit == commit_ack.commit), // Skip if there is already an ACK of this type and commit
        false => commit_acks
            .iter()
            .any(|c| c.ack_type == commit_ack.ack_type), // Skip if there is already an ACK of this type
    }
}

struct GitHubReviewComment {
    user: String,
    url: String,
    body: String,
    id: String,
}

async fn refresh_summary_comment(ctx: &Context, repo: Repository, pr_number: u64) -> Result<()> {
    let pr = ctx
        .octocrab
        .pulls(&repo.owner, &repo.name)
        .get(pr_number)
        .await?;

    let all_comments = ctx.octocrab.all_pages(ctx.octocrab.issues(&repo.owner, &repo.name)
        .list_comments(pr_number).send().await?).await?;

    let cmt = util::get_metadata_sections_from_comments(&all_comments, pr_number);
    
    let mut all_comments = all_comments.iter().map(|c| GitHubReviewComment {
        user: c.user.login.clone(),
        url: c.html_url.to_string().clone(),
        body: c.body.as_ref().unwrap_or(&String::new()).clone(),
        id: c.id.to_string(),
    }).collect::<Vec<GitHubReviewComment>>();
    let mut all_review_comments = ctx.octocrab.all_pages(ctx.octocrab.pulls(&repo.owner, &repo.name)
        .list_reviews(pr_number).await?)
        .await?.iter().map(|c| GitHubReviewComment {
            user: c.user.login.clone(),
            url: c.html_url.to_string().clone(),
            body: c.body.as_ref().unwrap_or(&String::new()).clone(),
            id: c.id.to_string(),
        }).collect::<Vec<GitHubReviewComment>>();

    all_comments.append(&mut all_review_comments);


    let mut ack_per_user: HashMap<String, Vec<AckCommit>> = HashMap::new(); // Need to store all acks per user to avoid duplicates
    let mut parsed_acks = Vec::new();

    println!("Comments count {}", all_comments.len());
    for comment in all_comments {
        // Check comment id is not the same as cmt.id wichi is optional
        if let Some(cmt_id) = &cmt.id {
            if cmt_id.to_string() == comment.id {
                continue;
            }
        }

        let acks = parse_acks_in_comment(&comment.body); // A single comment can contain multiple acks, e.g 'Concept ACK and Code Review ACK'

        for ack_commit in acks {
            let ack_type = ack_commit.ack_type.clone();
            let commit = ack_commit.commit.clone();

            if commit.is_none() && ack_type.requires_commit_hash() {
                // If the ack type requires a commit hash, but the comment does not contain one, skip it
                continue;
            }

            if ack_per_user.contains_key(&comment.user) // If the user already has an ack for this commit
                && should_skip_ack(ack_commit.clone(), ack_per_user[&comment.user].clone())
            // Maybe the ack is a duplicate, need to check for Stale acks
            {
                continue;
            }

            let head = commit.is_some() && commit.as_ref().map_or(false, |c| c.0 == pr.head.sha); // Check if the commit is the head commit of the PR

            parsed_acks.push(Review {
                user: comment.user.clone(),
                ack_type,
                commit: commit.map(|c| (c, head)),
                url: comment.url.clone(),
            });

            ack_per_user
                .entry(comment.user.clone())
                .or_insert_with(|| vec![ack_commit.clone()]);
        }
    }

    let comment = summary_comment_template(false, Some(parsed_acks));
    util::update_metadata_comment(
        &ctx.octocrab.issues(&repo.owner, &repo.name),
        cmt,
        &comment,
        util::IdComment::SecReviews,
        false).await?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum AckType {
    ACK,
    ConceptACK,
    ConceptNACK,
    ApproachACK,
    ApproachNACK,

    StaleACK, // ACK, but the commit is not the head of the PR anymore
}

impl AckType {
    fn requires_commit_hash(&self) -> bool {
        match self {
            AckType::ACK => true,
            _ => false,
        }
    }

    fn to_string(&self) -> &str {
        match self {
            AckType::ACK => "ACK",
            AckType::ConceptACK => "Concept ACK",
            AckType::ConceptNACK => "Concept NACK",
            AckType::ApproachACK => "Approach ACK",
            AckType::ApproachNACK => "Approach NACK",
            AckType::StaleACK => "Stale ACK",
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
        ["code review ack", "cr ack", "cr-ack", "crack"] => AckType::ACK;
        ["concept ack", "concept-ack", "conceptack"] => AckType::ConceptACK;
        ["concept nack", "concept-nack", "conceptnack"] => AckType::ConceptNACK;
        ["approach ack", "approach-ack", "approachack"] => AckType::ApproachACK;
        ["approach nack", "approach-nack", "approachnack"] => AckType::ApproachNACK;
        ["ack", "utack", "tack"] => AckType::ACK;
        ["nack"] => AckType::ConceptNACK
    ];
}
#[derive(Debug)]
struct Review {
    user: String,
    ack_type: AckType,
    commit: Option<(Commit, bool)>,
    url: String,
}

fn is_commit_hash(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, PartialEq, Clone, Hash, Eq)]
struct Commit(String);

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
struct AckCommit {
    ack_type: AckType,
    commit: Option<Commit>,
}

fn parse_acks_in_comment(comment: &str) -> Vec<AckCommit> {
    let comment = comment.to_lowercase();
    let words = comment
        .split("\n")
        .filter(|s| !s.starts_with(">")) // Ignore quoted text
        .flat_map(|s| s.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())) // Split on whitespace and punctuation
        .collect::<Vec<_>>(); // Collect into a Vec

    // Split words by whitespace and punctuation
    let mut acks = Vec::new();

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

                if pattern_word != &word{
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
                        commit = Some(Commit(next_word.to_string())); // If there is a commit hash, attach it to the ack
                    }

                    if ack_type.requires_commit_hash() && commit.is_none() {
                        // If the ack type requires a commit hash, but there is no commit hash, skip this pattern
                        continue;
                    }
                }

                acks.push(AckCommit {
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

    acks
}

// Test that parse_acks_in_comment works
#[cfg(test)]
mod tests {
    use super::*;

    struct TestCase {
        comment: &'static str,
        expected: Vec<AckCommit>,
    }

    #[test]
    fn test_parse_acks_in_comment() {
        let test_cases = vec![
            TestCase {
                comment: "ACK",
                expected: vec![],
            },
            TestCase {
                comment: "ACK 1234567890123456789012345678901234567890",
                expected: vec![AckCommit {
                    ack_type: AckType::ACK,
                    commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                }],
            },
            TestCase {
                comment: "ACK invalid",
                expected: vec![],
            },
            TestCase {
                comment: "ACK 1234567890123456789012345678901234567890 invalid",
                expected: vec![AckCommit {
                    ack_type: AckType::ACK,
                    commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                }],
            },
            TestCase {
                comment: "ACK 1234567890123456789012345678901234567890\nACK 1234567890123456789012345678901234567890",
                expected: vec![
                    AckCommit {
                        ack_type: AckType::ACK,
                        commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                    },
                    AckCommit {
                        ack_type: AckType::ACK,
                        commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                    },
                ],
            },
            TestCase {
                comment: "ACK 1234567890123456789012345678901234567890\nNACK 1234567890123456789012345678901234567890",
                expected: vec![
                    AckCommit {
                        ack_type: AckType::ACK,
                        commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                    },
                    AckCommit {
                        ack_type: AckType::ConceptNACK,
                        commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                    },
                ],
            },
            TestCase {
                comment: "Concept ACK",
                expected: vec![AckCommit {
                    ack_type: AckType::ConceptACK,
                    commit: None,
                }],
            },
            TestCase {
                comment: "Concept ACK 1234567890123456789012345678901234567890",
                expected: vec![AckCommit {
                    ack_type: AckType::ConceptACK,
                    commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                }],
            },
            TestCase {
                comment: "tACK",
                expected: vec![],
            },
            TestCase {
                comment: "tACK 1234567890123456789012345678901234567890",
                expected: vec![AckCommit {
                    ack_type: AckType::ACK,
                    commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                }],
            },
            TestCase {
                comment: "Code Review ACK 1234567890123456789012345678901234567890",
                expected: vec![AckCommit {
                    ack_type: AckType::ACK,
                    commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                }],
            },
            TestCase {
                comment: "crACK",
                expected: vec![],
            },
            TestCase {
                comment: "crACK 1234567890123456789012345678901234567890",
                expected: vec![AckCommit {
                    ack_type: AckType::ACK,
                    commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                }],
            },
            TestCase {
                comment: "Approach ACK",
                expected: vec![AckCommit {
                    ack_type: AckType::ApproachACK,
                    commit: None,
                }],
            },
            TestCase {
                comment: "Approach ACK 1234567890123456789012345678901234567890",
                expected: vec![AckCommit {
                    ack_type: AckType::ApproachACK,
                    commit: Some(Commit("1234567890123456789012345678901234567890".to_string())),
                }],
            },
            TestCase {
                comment: "Concept NACK",
                expected: vec![AckCommit {
                    ack_type: AckType::ConceptNACK,
                    commit: None,
                }],
            },
            TestCase {
                comment: "nack this change!",
                expected: vec![AckCommit {
                    ack_type: AckType::ConceptNACK,
                    commit: None,
                }],
            },
            TestCase {
                comment: "> Concept ACK",
                expected: vec![],
            },
            TestCase {
                comment: "This is a Concept ACK for me!",
                expected: vec![
                    AckCommit {
                        ack_type: AckType::ConceptACK,
                        commit: None,
                    },
                ],
            }
        ];

        for test_case in test_cases {
            let actual = parse_acks_in_comment(test_case.comment);
            println!("Test case: {}", test_case.comment);
            assert_eq!(actual, test_case.expected);
        }
    }
}
