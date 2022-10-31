use clap::Parser;

#[derive(clap::Parser)]
#[command(about = "\
Handle stale issues and pull requests:
* Comment on pull requests that needed a rebase for too long.\n\
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
    config_file: String,
    /// Print changes/edits instead of calling the GitHub API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(serde::Deserialize)]
struct Config {
    inactive_rebase_days: i64,
    inactive_rebase_comment: String,
    needs_rebase_label: String,
    needs_rebase_comment: String,
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
            let text = format!("{}\n{}", id_stale_comment, config.inactive_rebase_comment);
            if !dry_run {
                issues_api
                    .create_comment(item.number.try_into().unwrap(), text)
                    .await?;
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
    let id_stale_comment = util::IdComment::Stale.str();

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
            let mut labels = github
                .all_pages(issues_api.list_labels_for_issue(pull.number).send().await?)
                .await?;
            let found_label_rebase = labels
                .iter()
                .find(|l| l.name == config.needs_rebase_label)
                .is_some();
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
                                || b.starts_with(id_stale_comment)
                        })
                        .collect::<Vec<_>>();
                    println!("... delete {} comments", comments.len());
                    if !dry_run {
                        labels = issues_api
                            .remove_label(pull.number, &config.needs_rebase_label)
                            .await?;
                        for c in comments {
                            issues_api.delete_comment(c.id).await?;
                        }
                    }
                }
            } else {
                if !found_label_rebase {
                    println!("... add label '{}'", config.needs_rebase_label);
                    if !dry_run {
                        labels = issues_api
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
    rebase_label(&github, &config, &args.github_repo, args.dry_run).await?;

    Ok(())
}
