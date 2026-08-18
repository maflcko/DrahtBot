#[derive(Clone)]
pub struct Slug {
    pub owner: String,
    pub repo: String,
}

impl Slug {
    pub fn str(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

impl std::str::FromStr for Slug {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Format: a/b
        let err = "Wrong format, see --help.";
        let mut it_slug = s.split('/');
        let res = Self {
            owner: it_slug.next().ok_or(err)?.to_string(),
            repo: it_slug.next().ok_or(err)?.to_string(),
        };
        if it_slug.next().is_none() {
            return Ok(res);
        }
        Err(err)
    }
}

#[cfg(feature = "github")]
pub fn get_octocrab(token: Option<String>) -> octocrab::Result<octocrab::Octocrab> {
    let build = octocrab::Octocrab::builder();
    match token {
        Some(tok) => build.personal_token(tok),
        None => build,
    }
    .build()
}

#[cfg(feature = "github")]
pub enum IdComment {
    NeedsRebase,
    CiFailed,
    InactiveRebase,
    InactiveCi,
    InactiveStale,
    Metadata, // The "root" section
    SecCodeCoverage,
    SecConflicts,
    SecCoverage,
    SecReviews,
    SecLmCheck,
}

#[cfg(feature = "github")]
impl IdComment {
    pub fn str(&self) -> &'static str {
        match self {
            Self::NeedsRebase => "<!--cf906140f33d8803c4a75a2196329ecb-->",
            Self::CiFailed => "<!--85328a0da195eb286784d51f73fa0af9-->",
            Self::InactiveRebase => "<!--13523179cfe9479db18ec6c5d236f789-->",
            Self::InactiveCi => "<!--2e250dc3d92b2c9115b66051148d6e47-->",
            Self::InactiveStale => "<!--8ac04cdde196e94527acabf64b896448-->",
            Self::Metadata => "<!--e57a25ab6845829454e8d69fc972939a-->",
            Self::SecCodeCoverage => "<!--006a51241073e994b41acfe9ec718e94-->",
            Self::SecConflicts => "<!--174a7506f384e20aa4161008e828411d-->",
            Self::SecCoverage => "<!--2502f1a698b3751726fa55edcda76cd3-->",
            Self::SecReviews => "<!--021abf342d371248e50ceaed478a90ca-->",
            Self::SecLmCheck => "<!--5faf32d7da4f0f540f40219e4f7537a3-->",
        }
    }
}

pub fn git() -> std::process::Command {
    std::process::Command::new("git")
}

pub fn check_call(cmd: &mut std::process::Command) {
    let status = cmd.status().expect("command error");
    assert!(status.success());
}

pub fn call(cmd: &mut std::process::Command) -> bool {
    let out = cmd.output().expect("command error");
    out.status.success()
}

pub fn check_output(cmd: &mut std::process::Command) -> String {
    let out = cmd.output().expect("command error");
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .expect("invalid utf8")
        .trim()
        .to_string()
}

pub fn chdir(p: &std::path::Path) {
    std::env::set_current_dir(p).expect("chdir error")
}

/// Normalize a git diff for LLM consumption by dropping removed lines and rewriting hunk headers.
pub fn prepare_raw_diff_for_llm(diff: &str) -> String {
    diff.lines()
        .filter(|line| !line.starts_with('-')) // Drop needless lines to avoid confusion and reduce token use
        .map(|line| {
            if line.starts_with('@') {
                "@@ (hunk header) @@" // Rewrite hunk header to avoid typos in hunk header truncated by git
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Shared prompt that tells the LLM it will be given a git diff before receiving instructions.
pub const LLM_SHARED_PROMPT_DIFF: &str = r#"
Evaluate the following git diff according to the instructions that follow.
"#;

#[derive(Clone, Copy)]
pub struct LlmCheck {
    prompt: &'static str,
    pub magic_all_good: &'static str,
    pub topic: &'static str,
}

impl LlmCheck {
    pub fn prompt(&self) -> String {
        self.prompt.replace("{magic_all_good}", self.magic_all_good)
    }
}

/// Prompt encouraging the LLM to highlight typos in git diff documentation.
pub const LLM_PROMPT_TYPOS: &str = r#"
Identify and provide feedback on typographic or grammatical errors in the provided git diff comments or documentation, focusing exclusively on errors impacting comprehension.

- Flag only if a word is misspelled, or the English is clearly broken and the intended meaning cannot be understood without guessing.
- Ignore style preferences, such as Oxford comma, missing or superfluous commas, missing articles, and missing or inconsistent punctuation.
- Focus solely on added diff lines beginning with +.
- Address only code comments (for example C++ or Python comments) or documentation (for example inline strings or markdown files).

# Output Format

List each error with minimal context, followed by a very brief rationale:
- original -> suggested revision [brief explanation]

If none are found, state: "{magic_all_good}".
"#;

/// Prompt encouraging the LLM to highlight missing named args in git diff.
pub const LLM_PROMPT_NAMED_ARGS: &str = r#"
Check C++ and Python code in the provided git diff for function calls where integral literal values (e.g., 0, true) are used as arguments.

- Focus solely on added diff lines beginning with +.
- In C++: Look for function calls with literals as positional arguments. Recommend replacing `func(x, y, 0)` with `func(x, y, /*named_arg=*/0)`.
- In Python: Look for function calls with literals as positional arguments. Recommend replacing `func(x, y, 0)` with `func(x, y, named_arg=0)` if the argument is not already named or not already using keyword syntax.
- Only suggest a named arg if the meaning of the integral literal is ambiguous and not obvious from the context.
- Do not flag or suggest changes for arguments that are already named or where such a comment would be more confusing.
- Do not flag the first two arguments of a function call. Only flag the third and later arguments.
- Do not flag string literals.
- Do not flag literals in low-level/system/library calls when the literal's purpose is obvious and naming it would add noise, like std::max, std::format.
- Limit findings and suggestions to literals (do not suggest for variables or expressions).

# Output Format

List each location with minimal context. Only list the location, and do not suggest an arg name for the keyword:

- [function_call_signature] in [filename]

If none are found, state: "{magic_all_good}".
"#;

pub const LLM_PROMPT_CMP_MACROS: &str = r#"
Scan the provided git diff for test comparisons that rely on generic check macros or bare assertions instead of using the comparison-specific helpers.

- Focus only on added diff lines beginning with +.
- In C++ only look for the BOOST_CHECK_THROW Boost.Test macro that only check a generic exception type and not a detailed message, for example:
  * BOOST_CHECK_THROW(obj.write(), std::runtime_error) -> BOOST_CHECK_EXCEPTION(obj.write(), std::runtime_error, HasReason("the exact failure message"))
- Do not flag bare assert(...) checks or any other boost test macros in any C++ code.
- In Python, functional tests under test/functional/, look for bare assert statements using built‑in comparison operators where a helper is clearly more appropriate. Only the following helpers are available:
  * assert a is b → assert_equal(a, b)
  * assert a == b → assert_equal(a, b)
  * assert a is not b → assert_not_equal(a, b) [Note: Omit cases where one arg to assert_not_equal is a fixed constant, because a failure does not need to pretty-print the fixed constant twice]
  * assert a != b → assert_not_equal(a, b)
  * assert a > b → assert_greater_than(a, b)
  * assert a >= b → assert_greater_than_or_equal(a, b)
  * assert abs(v - vexp) < 0.00001 → assert_approx(v, vexp, vspan=...)
- In Python do not flag asserts on expressions that are guaranteed to evaluate to `bool`. E.g `a in b` or `a not in b`.
- Only flag instances where the intent is explicit and the specialized macro is clearly applicable to avoid noise.

# Output Format

List each location with a concise suggestion:
- [filename] snippet -> recommendation

If none are found, state: "{magic_all_good}".
"#;

pub static LLM_TYPOS: LlmCheck = LlmCheck {
    prompt: LLM_PROMPT_TYPOS,
    magic_all_good: "No typos were found",
    topic: "Possible typos and grammar issues:",
};

pub static LLM_NAMED_ARGS: LlmCheck = LlmCheck {
    prompt: LLM_PROMPT_NAMED_ARGS,
    magic_all_good: "No suggestions were found",
    topic: "Possible places where named args for integral literals may be used (e.g. `func(x, /*named_arg=*/0)` in C++, and `func(x, named_arg=0)` in Python):",
};

pub static LLM_CMP_MACROS: LlmCheck = LlmCheck {
    prompt: LLM_PROMPT_CMP_MACROS,
    magic_all_good: "No comparison macro suggestions were found.",
    topic:
        "Possible places where comparison-specific test macros should replace generic comparisons:",
};

/// Return all available LLM lint checks
pub fn all_llm_checks() -> Vec<LlmCheck> {
    vec![LLM_TYPOS, LLM_NAMED_ARGS, LLM_CMP_MACROS]
}

/// Construct the OpenAI chat payload used by llm clients that request diff checks.
pub fn make_llm_payload(diff: &str, check_prompt: String) -> serde_json::Value {
    serde_json::json!({
      "model": "gpt-5.4-mini",
      "messages": [
        {
          "role": "developer",
          "content": [
            {
              "type": "text",
              "text": LLM_SHARED_PROMPT_DIFF
            }
          ]
        },
        {
          "role": "user",
          "content": [
            {
              "type": "text",
              "text": diff
            },
            {
              "type": "text",
              "text": check_prompt
            }
          ]
        }
      ],
      "response_format": {
        "type": "text"
      },
      "reasoning_effort": "medium",
      "service_tier": "default",
      "store": true
    })
}

#[cfg(feature = "github")]
pub struct MetaComment {
    pull_num: u64,
    pub id: Option<octocrab::models::CommentId>,
    sections: Vec<String>,
}

#[cfg(feature = "github")]
impl MetaComment {
    pub fn has_section(&self, section_id: &IdComment) -> bool {
        let id = section_id.str();
        self.sections.iter().any(|s| s.starts_with(id))
    }

    fn join_metadata_comment(&mut self) -> String {
        self.sections.sort();
        let desc = "The following sections might be updated with supplementary metadata relevant to reviewers and maintainers.";
        format!(
            "{root_id}\n\n{desc}\n\n{sec}",
            root_id = IdComment::Metadata.str(),
            sec = self.sections.join("")
        )
    }

    fn update(&mut self, id: IdComment, new_text: &str) -> bool {
        // TODO. Maybe check if new_text contains "<!--" and reject it as Err("Must not contain
        // magic section split string").
        // Also, could reword the magic section split string to `<!---` (with 3 dashes, to avoid
        // normal use collisions?)
        let needle = id.str();
        let new_section = format!("{}{}", needle, new_text);
        for s in self.sections.iter_mut() {
            if s.starts_with(needle) {
                // Section exists
                let orig = s.split(needle).nth(1).unwrap();
                if orig == new_text {
                    // Section up to date
                    return false;
                }
                // Update section
                *s = new_section;
                return true;
            }
        }
        // Create missing section
        self.sections.push(new_section);
        true
    }
}

#[cfg(feature = "github")]
pub async fn get_metadata_sections(
    api: &octocrab::Octocrab,
    api_issues: &octocrab::issues::IssueHandler<'_>,
    pull_nr: u64,
) -> octocrab::Result<MetaComment> {
    let comments = api
        .all_pages(api_issues.list_comments(pull_nr).send().await?)
        .await?;

    Ok(get_metadata_sections_from_comments(&comments, pull_nr))
}

#[cfg(feature = "github")]
pub fn get_metadata_sections_from_comments(
    comments: &Vec<octocrab::models::issues::Comment>,
    pull_nr: u64,
) -> MetaComment {
    for c in comments {
        let b = c.body.as_ref().expect("remote api error");
        if b.starts_with(IdComment::Metadata.str()) {
            let sections = b
                .split("<!--")
                .skip(2)
                .map(|s| format!("<!--{}", s))
                .collect::<Vec<_>>();

            return MetaComment {
                pull_num: pull_nr,
                id: Some(c.id),
                sections,
            };
        }
    }
    MetaComment {
        pull_num: pull_nr,
        id: None,
        sections: Vec::new(),
    }
}

#[cfg(feature = "github")]
pub async fn update_metadata_comment(
    api_issues: &octocrab::issues::IssueHandler<'_>,
    comment: &mut MetaComment,
    text: &str,
    section: IdComment,
    dry_run: bool,
) -> octocrab::Result<()> {
    if !comment.update(section, text) {
        // Section up to date
        return Ok(());
    }
    if comment.id.is_none() {
        // Create new metadata comment
        let full_text = comment.join_metadata_comment();
        println!("... Create new metadata comment");
        if !dry_run {
            let c = api_issues
                .create_comment(comment.pull_num, full_text)
                .await?;

            comment.id = Some(c.id);
        }

        return Ok(());
    }
    let full_text = comment.join_metadata_comment();
    println!("... Update comment");
    if !dry_run {
        api_issues
            .update_comment(comment.id.unwrap(), full_text)
            .await?;
    }
    Ok(())
}

#[cfg(feature = "github")]
pub async fn get_pull_mergeable(
    api: &octocrab::pulls::PullRequestHandler<'_>,
    number: u64,
) -> octocrab::Result<Option<octocrab::models::pulls::PullRequest>> {
    // https://docs.github.com/en/rest/guides/getting-started-with-the-git-database-api#checking-mergeability-of-pull-requests
    loop {
        let pull = api.get(number).await?;
        if pull.state.as_ref().unwrap() != &octocrab::models::IssueState::Open {
            return Ok(None);
        }
        if pull.mergeable.is_none() {
            std::thread::sleep(std::time::Duration::from_secs(3));
            continue;
        }
        return Ok(Some(pull));
    }
}
