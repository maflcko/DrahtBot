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
}

#[tokio::main]
async fn main() -> octocrab::Result<()> {
    let id_stale_comment = util::IdComment::Stale.str();

    let args = Args::parse();
    let config: Config = serde_yaml::from_reader(
        std::fs::File::open(args.config_file).expect("config file path error"),
    )
    .expect("yaml error");

    let github = util::get_octocrab(args.github_access_token)?;

    let cutoff =
        { chrono::Utc::now() - chrono::Duration::days(config.inactive_rebase_days) }.format("%F");
    println!("Mark stale before date {} ...", cutoff);

    for util::Slug { owner, repo } in args.github_repo {
        println!("Get stale pull requests for {owner}/{repo} ...");
        let items = github
            .all_pages(
                github
                    .search()
                    .issues_and_pull_requests(&format!(
                        "repo:{owner}/{repo} is:open is:pr label:\"Needs rebase\" updated:<={cutoff}"
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
            let text = id_stale_comment.to_owned()
                + "There hasn't been much activity lately and the patch still needs rebase. What is the status here?\n"
                + "\n"
                + "* Is it still relevant? ➡️ Please solve the conflicts to make it ready for review and to ensure the CI passes.\n"
                + "* Is it no longer relevant? ➡️ Please close.\n"
                + "* Did the author lose interest or time to work on this? ➡️ Please close it and mark it 'Up for grabs' with the label, so that it can be picked up in the future.\n";
            if !args.dry_run {
                issues_api
                    .create_comment(item.number.try_into().unwrap(), text)
                    .await?;
            }
        }
    }
    Ok(())
}
