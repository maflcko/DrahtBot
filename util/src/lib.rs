#[derive(Clone)]
pub struct Slug {
    pub owner: String,
    pub repo: String,
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

pub enum IdComment {
    NeedsRebase,
    ReviewersRequested,
    Stale,
    Metadata, // The "root" section
    SecConflicts,
    SecCoverage,
}

impl IdComment {
    pub fn str(self: Self) -> &'static str {
        match self {
            Self::NeedsRebase => "<!--cf906140f33d8803c4a75a2196329ecb-->",
            Self::ReviewersRequested => "<!--4a62be1de6b64f3ed646cdc7932c8cf5-->",
            Self::Stale => "<!--13523179cfe9479db18ec6c5d236f789-->",
            Self::Metadata => "<!--e57a25ab6845829454e8d69fc972939a-->",
            Self::SecConflicts => "<!--174a7506f384e20aa4161008e828411d-->",
            Self::SecCoverage => "<!--2502f1a698b3751726fa55edcda76cd3-->",
        }
    }
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
