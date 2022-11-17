use clap::Parser;
use std::str::FromStr;

#[derive(clap::Parser)]
#[command(about = "Update the pull request with missing labels.", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The path to the yaml config file.
    #[arg(long)]
    config_file: std::path::PathBuf,
    /// Print changes/edits instead of calling the GitHub API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(serde::Deserialize)]
struct Repo {
    repo_slug: String,
    backport_label: String,
    repo_labels: std::collections::HashMap<String, Vec<String>>,
}

#[derive(serde::Deserialize)]
struct Config {
    apply_labels: Vec<Repo>,
}

async fn apply_labels(
    github: &octocrab::Octocrab,
    config: &Config,
    dry_run: bool,
) -> octocrab::Result<()> {
    println!("Apply labels ...");

    for config_repo in &config.apply_labels {
        let util::Slug { owner, repo } =
            util::Slug::from_str(&config_repo.repo_slug).expect("config format error");
        let regs = config_repo.repo_labels.iter().fold(
            std::collections::HashMap::<&String, Vec<regex::Regex>>::new(),
            |mut acc, (label_name, title_regs)| {
                for reg in title_regs {
                    acc.entry(label_name).or_default().push(
                        regex::RegexBuilder::new(reg)
                            .case_insensitive(true)
                            .build()
                            .expect("regex config format error"),
                    );
                }
                acc
            },
        );
        println!("Repo {}/{} ...", owner, repo);
        let base_name = github
            .repos(&owner, &repo)
            .get()
            .await?
            .default_branch
            .expect("remote api error");
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
            let pull_title = pull.title.as_ref().expect("remote api error");
            let labels = github
                .all_pages(issues_api.list_labels_for_issue(pull.number).send().await?)
                .await?;
            if !labels.is_empty() {
                continue;
            }
            let mut new_labels = Vec::new();
            if pull.base.ref_field != base_name {
                new_labels.push(config_repo.backport_label.to_string());
            } else {
                for (label_name, title_regs) in &regs {
                    if title_regs.iter().any(|r| r.is_match(pull_title)) {
                        new_labels.push(label_name.to_string());
                        break;
                    }
                }
            }
            if new_labels.is_empty() {
                continue;
            }
            println!(" ... add_to_labels({:?})", new_labels);
            if !dry_run {
                issues_api.add_labels(pull.number, &new_labels).await?;
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

    apply_labels(&github, &config, args.dry_run).await?;

    Ok(())
}
