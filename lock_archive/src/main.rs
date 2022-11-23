use clap::Parser;

#[derive(clap::Parser)]
#[command(about = "Lock discussion on inactive closed issues and pull requests.", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the remotes on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: Vec<util::Slug>,
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

    let github = util::get_octocrab(args.github_access_token)?;

    let cutoff = { chrono::Utc::now() - chrono::Duration::days(args.inactive_days) }.format("%F");
    println!("Locking before date {} ...", cutoff);

    for util::Slug { owner, repo } in args.github_repo {
        println!("Get closed issues and pull requests for {owner}/{repo} ...");
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
                issues_api.lock(item.number, None).await?;
            }
        }
    }
    Ok(())
}
