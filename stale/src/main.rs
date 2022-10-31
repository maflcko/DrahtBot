use clap::Parser;

#[derive(clap::Parser)]
#[command(about = "Comment on pull requests that needed a rebase for too long.", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the remotes on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: Vec<util::Slug>,
    /// The path to the yaml config file.
    #[arg(long)]
    config_file: String,
    /// Print changes/edits instead of calling the GitHub API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(serde::Deserialize)]
struct Config {
    inactive_rebase_days: i64,
    inactive_rebase_comment: String,
    label_needs_rebase: String,
}

async fn stale(
    github: &octocrab::Octocrab,
    config: &Config,
    github_repo: &Vec<util::Slug>,
    dry_run: bool,
) -> octocrab::Result<()> {
    let id_stale_comment = util::IdComment::Stale.str();

    let cutoff =
        { chrono::Utc::now() - chrono::Duration::days(config.inactive_rebase_days) }.format("%F");
    println!("Mark stale before date {} ...", cutoff);

    for util::Slug { owner, repo } in github_repo {
        println!("Get stale pull requests for {owner}/{repo} ...");
        let search_fmt = format!(
            "repo:{owner}/{repo} is:open is:pr label:\"{label}\" updated:<={cutoff}",
            owner = owner,
            repo = repo,
            label = config.label_needs_rebase,
            cutoff = cutoff
        );
        let items = github
            .all_pages(
                github
                    .search()
                    .issues_and_pull_requests(&search_fmt)
                    .send()
                    .await?,
            )
            .await?;
        let issues_api = github.issues(owner, repo);
        for (i, item) in items.iter().enumerate() {
            println!(
                "{}/{} (Item: {}/{}#{})",
                i,
                items.len(),
                owner,
                repo,
                item.number,
            );
            let text = id_stale_comment.to_owned() + &config.inactive_rebase_comment;
            if !dry_run {
                issues_api
                    .create_comment(item.number.try_into().unwrap(), text)
                    .await?;
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> octocrab::Result<()> {
    let args = Args::parse();
    let config: Config = serde_yaml::from_reader(
        std::fs::File::open(args.config_file).expect("config file path error"),
    )
    .expect("yaml error");

    let github = util::get_octocrab(args.github_access_token)?;

    stale(&github, &config, &args.github_repo, args.dry_run).await?;

    Ok(())
}
