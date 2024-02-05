use clap::Parser;

#[derive(clap::Parser)]
#[command(about = "\
Handle stale issues and pull requests:
* Comment on pull requests that needed a rebase for too long.\n\
* Comment on pull requests that a failing CI for too long.\n\
* Comment on pull requests that are inactive for too long.\n\
* Update the label that indicates a rebase is required.\n\
", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the remotes on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: Vec<util::Slug>,
    /// The path to the yaml config file.
    #[arg(long)]
    config_file: std::path::PathBuf,
    /// Print changes/edits instead of calling the GitHub API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(serde::Deserialize)]
struct Config {
    inactive_rebase_days: i64,
    inactive_rebase_comment: String,
    inactive_ci_days: i64,
    inactive_ci_comment: String,
    inactive_stale_days: i64,
    inactive_stale_comment: String,
    needs_rebase_label: String,
    ci_failed_label: String,
    needs_rebase_comment: String,
}

async fn inactive_rebase(
    github: &octocrab::Octocrab,
    config: &Config,
    github_repo: &Vec<util::Slug>,
    dry_run: bool,
) -> octocrab::Result<()> {
    let id_inactive_rebase_comment = util::IdComment::InactiveRebase.str();

    let cutoff =
        { chrono::Utc::now() - chrono::Duration::days(config.inactive_rebase_days) }.format("%F");
    println!("Mark inactive_rebase before date {} ...", cutoff);

    for util::Slug { owner, repo } in github_repo {
        println!("Get inactive_rebase pull requests for {owner}/{repo} ...");
        let search_fmt = format!(
            "repo:{owner}/{repo} is:open is:pr label:\"{label}\" updated:<={cutoff}",
            owner = owner,
            repo = repo,
            label = config.needs_rebase_label,
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
            let text = format!(
                "{}\n{}",
                id_inactive_rebase_comment, config.inactive_rebase_comment
            );
            if !dry_run {
                issues_api.create_comment(item.number, text).await?;
            }
        }
    }
    Ok(())
}

async fn inactive_ci(
    github: &octocrab::Octocrab,
    config: &Config,
    github_repo: &Vec<util::Slug>,
    dry_run: bool,
) -> octocrab::Result<()> {
    let id_inactive_ci_comment = util::IdComment::InactiveCi.str();

    let cutoff =
        { chrono::Utc::now() - chrono::Duration::days(config.inactive_ci_days) }.format("%F");
    println!("Mark inactive_ci before date {} ...", cutoff);

    for util::Slug { owner, repo } in github_repo {
        println!("Get inactive_ci pull requests for {owner}/{repo} ...");
        let search_fmt = format!(
            "repo:{owner}/{repo} is:open is:pr label:\"{label}\" updated:<={cutoff}",
            owner = owner,
            repo = repo,
            label = config.ci_failed_label,
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
            let text = format!("{}\n{}", id_inactive_ci_comment, config.inactive_ci_comment);
            if !dry_run {
                issues_api.create_comment(item.number, text).await?;
            }
        }
    }
    Ok(())
}

async fn inactive_stale(
    github: &octocrab::Octocrab,
    config: &Config,
    github_repo: &Vec<util::Slug>,
    dry_run: bool,
) -> octocrab::Result<()> {
    let id_inactive_stale_comment = util::IdComment::InactiveStale.str();

    let cutoff =
        { chrono::Utc::now() - chrono::Duration::days(config.inactive_stale_days) }.format("%F");
    println!("Mark inactive_stale before date {} ...", cutoff);

    for util::Slug { owner, repo } in github_repo {
        println!("Get inactive_stale pull requests for {owner}/{repo} ...");
        let search_fmt = format!(
            "repo:{owner}/{repo} is:open is:pr updated:<={cutoff}",
            owner = owner,
            repo = repo,
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
            let text = format!(
                "{}\n{}",
                id_inactive_stale_comment,
                config
                    .inactive_stale_comment
                    .replace("{owner}", owner)
                    .replace("{repo}", repo)
            );
            if !dry_run {
                issues_api.create_comment(item.number, text).await?;
            }
        }
    }
    Ok(())
}

async fn rebase_label(
    github: &octocrab::Octocrab,
    config: &Config,
    github_repo: &Vec<util::Slug>,
    dry_run: bool,
) -> octocrab::Result<()> {
    let id_needs_rebase_comment = util::IdComment::NeedsRebase.str();
    let id_inactive_rebase_comment = util::IdComment::InactiveRebase.str();
    let id_inactive_stale_comment = util::IdComment::InactiveStale.str();

    println!("Apply rebase label");

    for util::Slug { owner, repo } in github_repo {
        println!("Get open pulls for {}/{} ...", owner, repo);
        let issues_api = github.issues(owner, repo);
        let pulls_api = github.pulls(owner, repo);
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
            let pull = util::get_pull_mergeable(&pulls_api, pull.number).await?;
            let pull = match pull {
                None => {
                    continue;
                }
                Some(p) => p,
            };
            let labels = github
                .all_pages(issues_api.list_labels_for_issue(pull.number).send().await?)
                .await?;
            let found_label_rebase = labels
                .into_iter()
                .any(|l| l.name == config.needs_rebase_label);
            if pull.mergeable.unwrap() {
                if found_label_rebase {
                    println!("... remove label '{}')", config.needs_rebase_label);
                    let all_comments = github
                        .all_pages(issues_api.list_comments(pull.number).send().await?)
                        .await?;
                    let comments = all_comments
                        .iter()
                        .filter(|c| {
                            let b = c.body.as_ref().unwrap();
                            b.starts_with(id_needs_rebase_comment)
                                || b.starts_with(id_inactive_rebase_comment)
                                || b.starts_with(id_inactive_stale_comment)
                        })
                        .collect::<Vec<_>>();
                    println!("... delete {} comments", comments.len());
                    if !dry_run {
                        issues_api
                            .remove_label(pull.number, &config.needs_rebase_label)
                            .await?;
                        for c in comments {
                            issues_api.delete_comment(c.id).await?;
                        }
                    }
                }
            } else if !found_label_rebase {
                println!("... add label '{}'", config.needs_rebase_label);
                if !dry_run {
                    issues_api
                        .add_labels(pull.number, &[config.needs_rebase_label.to_string()])
                        .await?;
                    let text = format!(
                        "{}\n{}",
                        id_needs_rebase_comment,
                        config
                            .needs_rebase_comment
                            .replace("{owner}", owner)
                            .replace("{repo}", repo)
                    );
                    issues_api.create_comment(pull.number, text).await?;
                }
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

    inactive_rebase(&github, &config, &args.github_repo, args.dry_run).await?;
    inactive_ci(&github, &config, &args.github_repo, args.dry_run).await?;
    inactive_stale(&github, &config, &args.github_repo, args.dry_run).await?;
    rebase_label(&github, &config, &args.github_repo, args.dry_run).await?;

    Ok(())
}
