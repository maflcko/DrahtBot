use clap::Parser;

#[derive(Clone)]
struct Slug {
    owner: String,
    repo: String,
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

enum IdComment {
    NeedsRebase,
    ReviewersRequested,
    Stale,
    Metadata, // The "root" section
    SecConflicts,
    SecCoverage,
}

impl IdComment {
    fn str(self: Self) -> &'static str {
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

#[derive(clap::Parser)]
#[command(about = "Update the label that indicates a rebase is required.", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the remotes on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: Vec<Slug>,
    /// Print changes/edits instead of calling the GitHub API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

async fn get_pull_mergeable(
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

#[tokio::main]
async fn main() -> octocrab::Result<()> {
    let id_needs_rebase_comment = IdComment::NeedsRebase.str();
    let id_stale_comment = IdComment::Stale.str();

    let label_needs_rebase = "Needs rebase";

    let args = Args::parse();

    let github = {
        let build = octocrab::Octocrab::builder();
        match args.github_access_token {
            Some(tok) => build.personal_token(tok),
            None => build,
        }
        .build()?
    };

    for Slug { owner, repo } in args.github_repo {
        println!("Get open pulls for {}/{} ...", owner, repo);
        let issues_api = github.issues(&owner, &repo);
        let pulls_api = github.pulls(&owner, &repo);
        let pulls = github
            .all_pages(
                pulls_api
                    .list()
                    .state(octocrab::params::State::Open)
                    .send()
                    .await?,
            )
            .await?;
        println!("Open pulls: {}", pulls.len());
        for (i, pull) in pulls.iter().enumerate() {
            println!(
                "{}/{} (Pull: {}/{}#{})",
                i,
                pulls.len(),
                owner,
                repo,
                pull.number
            );
            let pull = get_pull_mergeable(&pulls_api, pull.number).await?;
            let pull = match pull {
                None => {
                    continue;
                }
                Some(p) => p,
            };
            // let issue = issues_api.get(pull.number).await?;
            let mut labels = github
                .all_pages(issues_api.list_labels_for_issue(pull.number).send().await?)
                .await?;
            let found_label_rebase = labels
                .iter()
                .find(|l| l.name == label_needs_rebase)
                .is_some();
            if pull.mergeable.unwrap() {
                if found_label_rebase {
                    println!("... remove label '{}')", label_needs_rebase);
                    let all_comments = github
                        .all_pages(issues_api.list_comments(pull.number).send().await?)
                        .await?;
                    let comments = all_comments
                        .iter()
                        .filter(|c| {
                            let b = c.body.as_ref().unwrap();
                            b.starts_with(id_needs_rebase_comment)
                                || b.starts_with(id_stale_comment)
                        })
                        .collect::<Vec<_>>();
                    println!("... delete {} comments", comments.len());
                    if !args.dry_run {
                        labels = issues_api
                            .remove_label(pull.number, label_needs_rebase)
                            .await?;
                        for c in comments {
                            issues_api.delete_comment(c.id).await?;
                        }
                    }
                }
            } else {
                if !found_label_rebase {
                    println!("... add label '{}'", label_needs_rebase);
                    if !args.dry_run {
                        issues_api
                            .add_labels(pull.number, &[label_needs_rebase.to_string()])
                            .await?;
                        let text =id_needs_rebase_comment.to_owned() 
                        + "\n🐙 This pull request conflicts with the target branch and [needs rebase](https://github.com/bitcoin/bitcoin/blob/master/CONTRIBUTING.md#rebasing-changes).\n";
                        issues_api.create_comment(pull.number, text)
                    }
                }
            }
        }
    }
    Ok(())
}
