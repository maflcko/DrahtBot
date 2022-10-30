use clap::Parser;

#[derive(clap::Parser)]
#[command(about = "Update the label that indicates a rebase is required.", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the remotes on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: Vec<util::Slug>,
    /// Print changes/edits instead of calling the GitHub API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> octocrab::Result<()> {
    let id_needs_rebase_comment = util::IdComment::NeedsRebase.str();
    let id_stale_comment = util::IdComment::Stale.str();

    let label_needs_rebase = "Needs rebase";

    let args = Args::parse();

    let github = util::get_octocrab(args.github_access_token)?;

    for util::Slug { owner, repo } in args.github_repo {
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
                        labels = issues_api
                            .add_labels(pull.number, &[label_needs_rebase.to_string()])
                            .await?;
                        let text = format!(
                            "{} \n\
                            🐙 This pull request conflicts with \
                            the target branch and [needs rebase]\
                            (https://github.com/{}/{}/blob/master\
                            /CONTRIBUTING.md#rebasing-changes).",
                            id_needs_rebase_comment.to_owned(),
                            &owner,
                            &repo
                        );
                        issues_api.create_comment(pull.number, text).await?;
                    }
                }
            }
        }
    }
    Ok(())
}
