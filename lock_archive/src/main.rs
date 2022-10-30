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

#[derive(clap::Parser)]
#[command(about = "Lock discussion on inactive closed issues and pull requests.", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the remotes on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: Vec<Slug>,
    /// Lock a closed issue or pull request after this many days of inactivity
    #[arg(long, default_value_t = 365)]
    inactive_days: i64,
    /// Print changes/edits instead of calling the GitHub API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> octocrab::Result<()> {
    let args = Args::parse();

    let github = {
        let build = octocrab::Octocrab::builder();
        match args.github_access_token {
            Some(tok) => build.personal_token(tok),
            None => build,
        }
        .build()?
    };

    let cutoff = (chrono::Utc::now() - chrono::Duration::days(args.inactive_days)).format("%F");
    println!("Locking before date {} ...", cutoff);

    for Slug { owner, repo } in args.github_repo {
        println!(
            "Get closed issues and pull requests for {}/{} ...",
            owner, repo
        );
        let items = github
            .all_pages(
                github
                    .search()
                    .issues_and_pull_requests(&format!(
                        "repo:{owner}/{repo} is:unlocked is:closed updated:<={cutoff}"
                    ))
                    .send()
                    .await?,
            )
            .await?;
        let issues_api = github.issues(&owner, &repo);
        println!("Items: {}", items.len());
        for (i, item) in items.iter().enumerate() {
            println!(
                "{}/{} (Item: {}/{}#{})",
                i,
                items.len(),
                owner,
                repo,
                item.number,
            );
            if !args.dry_run {
                issues_api
                    .lock(item.number.try_into().unwrap(), None)
                    .await?;
            }
        }
    }
    Ok(())
}
