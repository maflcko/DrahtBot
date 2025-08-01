use clap::Parser;
use octocrab::params::repos::Commitish;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

#[derive(clap::Parser)]
#[command(about = "Trigger GHA CI to re-run.", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the remotes on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: Vec<util::Slug>,
    /// The task names to re-run.
    #[arg(long)]
    task: Vec<String>,
    /// How many minutes to sleep between pull re-runs.
    #[arg(long, default_value_t = 25)]
    sleep_min: u64,
    /// Print changes/edits instead of calling the GitHub/CI API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

async fn rerun_first(
    owner: &str,
    repo: &str,
    token: &str,
    task_name: &str,
    check_runs: &[octocrab::models::checks::CheckRun],
    dry_run: bool,
) -> octocrab::Result<()> {
    if let Some(task) = check_runs.iter().find(|t| t.name.contains(task_name)) {
        println!("Re-run task {n} (id: {i})", n = task.name, i = task.id);
        if !dry_run {
            util::check_call(std::process::Command::new("curl").args([
                "-L",
                "-X",
                "POST",
                "-H",
                "Accept: application/vnd.github+json",
                "-H",
                &format!("Authorization: Bearer {token}",),
                "-H",
                "X-GitHub-Api-Version: 2022-11-28",
                &format!(
                    "https://api.github.com/repos/{owner}/{repo}/actions/jobs/{id}/rerun",
                    id = task.id
                ),
            ]));
            // Ignore result, but log it. May fail if the task is older than 30 days.
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> octocrab::Result<()> {
    let args = Args::parse();

    let github = util::get_octocrab(args.github_access_token.clone())?;

    for util::Slug { owner, repo } in args.github_repo {
        println!("Get open pulls for {owner}/{repo} ...");
        let pulls_api = github.pulls(&owner, &repo);
        let checks_api = github.checks(&owner, &repo);
        let pulls = {
            let mut pulls = github
                .all_pages(
                    pulls_api
                        .list()
                        .state(octocrab::params::State::Open)
                        .send()
                        .await?,
                )
                .await?;
            // Rotate the vector to start at a different place each time, to account for
            // api.cirrus-ci network errors, which would abort the program. On the next start, it
            // would start iterating from the same place.
            let rotate = RandomState::new().build_hasher().finish() as usize % (pulls.len());
            pulls.rotate_left(rotate);
            pulls
        };
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
            if !pull.mergeable.unwrap() {
                continue;
            }
            let check_runs = checks_api
                .list_check_runs_for_git_ref(Commitish(pull.head.sha.to_string()))
                .per_page(90)
                .send()
                .await?
                .check_runs;
            for task_name in &args.task {
                if let Err(msg) = rerun_first(
                    &owner,
                    &repo,
                    args.github_access_token
                        .as_deref()
                        .unwrap_or("missing_token"),
                    task_name,
                    &check_runs,
                    args.dry_run,
                )
                .await
                {
                    println!("{msg:?}");
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(args.sleep_min * 60));
        }
    }
    Ok(())
}
