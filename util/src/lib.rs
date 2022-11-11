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

pub fn get_octocrab(token: Option<String>) -> octocrab::Result<octocrab::Octocrab> {
    let build = octocrab::Octocrab::builder();
    match token {
        Some(tok) => build.personal_token(tok),
        None => build,
    }
    .build()
}

pub enum IdComment {
    NeedsRebase,
    ReviewersRequested,
    InactiveRebase,
    InactiveStale,
    Metadata, // The "root" section
    SecConflicts,
    SecCoverage,
    SecReviews,
}

impl IdComment {
    pub fn str(&self) -> &'static str {
        match self {
            Self::NeedsRebase => "<!--cf906140f33d8803c4a75a2196329ecb-->",
            Self::ReviewersRequested => "<!--4a62be1de6b64f3ed646cdc7932c8cf5-->",
            Self::InactiveRebase => "<!--13523179cfe9479db18ec6c5d236f789-->",
            Self::InactiveStale => "<!--8ac04cdde196e94527acabf64b896448-->",
            Self::Metadata => "<!--e57a25ab6845829454e8d69fc972939a-->",
            Self::SecConflicts => "<!--174a7506f384e20aa4161008e828411d-->",
            Self::SecCoverage => "<!--2502f1a698b3751726fa55edcda76cd3-->",
            Self::SecReviews => "<!--021abf342d371248e50ceaed478a90ca-->",
        }
    }
}

pub fn git() -> std::process::Command {
    std::process::Command::new("git")
}

pub fn check_call(cmd: &mut std::process::Command) {
    check_output(cmd);
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

pub struct MetaComment {
    pull_num: u64,
    pub id: Option<octocrab::models::CommentId>,
    sections: Vec<String>,
}

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

pub async fn update_metadata_comment(
    api_issues: &octocrab::issues::IssueHandler<'_>,
    mut comment: MetaComment,
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
            api_issues
                .create_comment(comment.pull_num, full_text)
                .await?;
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

pub async fn get_pulls_mergeable(
    api: &octocrab::Octocrab,
    api_pulls: &octocrab::pulls::PullRequestHandler<'_>,
    base_name: &str,
) -> octocrab::Result<Vec<octocrab::models::pulls::PullRequest>> {
    let mut pulls = api
        .all_pages(
            api_pulls
                .list()
                .state(octocrab::params::State::Open)
                .base(base_name)
                .send()
                .await?,
        )
        .await?;
    while pulls.iter().any(|p| p.mergeable.is_none()) {
        std::thread::sleep(std::time::Duration::from_secs(3));
        pulls = futures::future::join_all(
            pulls
                .into_iter()
                .filter(|p| {
                    p.state.as_ref().expect("remote api error")
                        == &octocrab::models::IssueState::Open
                })
                .map(|p| async {
                    if p.mergeable.is_none() {
                        api_pulls.get(p.number).await.expect("remote api error")
                    } else {
                        p
                    }
                })
                .collect::<Vec<_>>(),
        )
        .await;
    }
    Ok(pulls)
}

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
