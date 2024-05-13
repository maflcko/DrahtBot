use clap::Parser;
use std::io::Write;

#[derive(clap::Parser)]
#[command(about = "Determine conflicting pull requests.", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the monotree remotes on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: Vec<util::Slug>,
    /// Update the conflict comment and label for this pull request. Format: owner/repo/number
    #[arg(long, value_parser=parse_pull_id)]
    pull_id: Option<String>,
    /// Update all conflicts comments and labels.
    #[arg(long, default_value_t = false)]
    update_comments: bool,
    /// The local dir used for scratching.
    #[arg(long)]
    scratch_dir: std::path::PathBuf,
    /// The path to the yaml config file.
    #[arg(long)]
    config_file: std::path::PathBuf,
    /// Print changes/edits instead of calling the GitHub API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

fn parse_pull_id(val: &str) -> Result<String, String> {
    if 3 == val.split('/').count() {
        return Ok(val.to_string());
    }
    Err("".to_string())
}

#[derive(serde::Deserialize)]
struct Config {
    conflicts_heading: String,
    conflicts_description: String,
    conflicts_empty: String,
}

fn init_git(monotree_dir: &std::path::Path, repos: &Vec<util::Slug>) {
    if monotree_dir.is_dir() {
        return;
    }
    for sl in repos {
        let sl = sl.str();
        let url = format!("https://github.com/{sl}");
        println!("Clone {url} repo to {dir}", dir = monotree_dir.display());
        if !monotree_dir.is_dir() {
            util::check_call(
                util::git()
                    .args(["clone", "--quiet", &url])
                    .arg(monotree_dir),
            );
        }
        println!("Set git metadata");
        util::chdir(monotree_dir);
        {
            let err = "git config file error";
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(monotree_dir.join(".git").join("config"))
                .expect(err);
            writeln!(f, "[remote \"con_pull_ref/{sl}\"]").expect(err);
            writeln!(f, "    url = {url}").expect(err);
            writeln!(f, "    fetch = +refs/pull/*:refs/remotes/upstream-pull/*").expect(err);
        }
        util::check_call(util::git().args(["config", "fetch.showForcedUpdates", "false"]));
        util::check_call(util::git().args(["config", "user.email", "no@ne.nl"]));
        util::check_call(util::git().args(["config", "user.name", "none"]));
        util::check_call(util::git().args(["config", "gc.auto", "0"]));
    }
}

struct MetaPull {
    pull: octocrab::models::pulls::PullRequest,
    head_commit: String,
    slug: util::Slug,
    slug_num: String,
    merge_commit: Option<String>,
}

fn merge_strategy() -> &'static str {
    // https://github.blog/changelog/2022-09-12-merge-commits-now-created-using-the-merge-ort-strategy/
    "--strategy=ort"
}

fn calc_mergeable(pulls: Vec<MetaPull>, base_branch: &str) -> Vec<MetaPull> {
    let base_id = util::check_output(
        util::git()
            .args(["log", "-1", "--format=%H"])
            .arg(format!("origin/{base_branch}")),
    );
    let mut ret = Vec::new();
    for mut p in pulls {
        util::check_call(util::git().args(["checkout", &base_id, "--quiet"]));
        let mergeable = util::call(
            util::git()
                .args(["merge", merge_strategy(), "--quiet", &p.head_commit, "-m"])
                .arg(format!("Prepare base for {id}", id = p.slug_num)),
        );

        if mergeable {
            p.merge_commit = Some(util::check_output(util::git().args([
                "log",
                "-1",
                "--format=%H",
                "HEAD",
            ])));
            ret.push(p);
        } else {
            util::check_call(util::git().args(["merge", "--abort"]));
        }
    }
    ret
}

fn calc_conflicts<'a>(
    pulls_mergeable: &'a Vec<MetaPull>,
    pull_check: &MetaPull,
) -> Vec<&'a MetaPull> {
    let mut conflicts = Vec::new();
    let base_id = util::check_output(util::git().args([
        "log",
        "-1",
        "--format=%H",
        pull_check.merge_commit.as_ref().expect("merge id missing"),
    ]));
    for pull_other in pulls_mergeable {
        if pull_check.slug_num == pull_other.slug_num {
            continue;
        }
        util::check_call(util::git().args(["checkout", &base_id, "--quiet"]));
        if !util::call(
            util::git()
                .args([
                    "merge",
                    merge_strategy(),
                    "--quiet",
                    &pull_other.head_commit,
                    "-m",
                ])
                .arg(format!(
                    "Merge base_{pr_id}+{pr_o_id}",
                    pr_id = pull_check.slug_num,
                    pr_o_id = pull_other.slug_num
                )),
        ) {
            util::check_call(util::git().args(["merge", "--abort"]));
            conflicts.push(pull_other);
        }
    }
    conflicts
}

async fn update_comment(
    config: &Config,
    api: &octocrab::Octocrab,
    dry_run: bool,
    pull: &MetaPull,
    pulls_conflict: &[&MetaPull],
) -> octocrab::Result<()> {
    let api_issues = api.issues(&pull.slug.owner, &pull.slug.repo);
    let mut cmt = util::get_metadata_sections(api, &api_issues, pull.pull.number).await?;
    if pulls_conflict.is_empty() {
        if cmt.id.is_none() || !cmt.has_section(&util::IdComment::SecConflicts) {
            // No conflict and no section to update
            return Ok(());
        }
        // Update section for no conflicts
        util::update_metadata_comment(
            &api_issues,
            &mut cmt,
            &format!(
                "\n### {hd}\n{txt}",
                hd = config.conflicts_heading,
                txt = config.conflicts_empty,
            ),
            util::IdComment::SecConflicts,
            dry_run,
        )
        .await?;
        return Ok(());
    }

    util::update_metadata_comment(
        &api_issues,
        &mut cmt,
        &format!(
            "\n### {hd}\n{txt}",
            hd = config.conflicts_heading,
            txt = config.conflicts_description.replace(
                "{conflicts}",
                &pulls_conflict
                    .iter()
                    .map(|p| format!(
                        "\n* [#{sn}]({url}) ({title} by {user})",
                        sn = p
                            .slug_num
                            .trim_start_matches(&format!("{sl}/", sl = pull.slug.str())),
                        url = p.pull.html_url.as_ref().expect("remote api error"),
                        title = p.pull.title.as_ref().expect("remote api error").trim(),
                        user = p.pull.user.as_ref().expect("remote api error").login
                    ))
                    .collect::<Vec<_>>()
                    .join("")
            )
        ),
        util::IdComment::SecConflicts,
        dry_run,
    )
    .await?;
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

    std::fs::create_dir_all(&args.scratch_dir).expect("invalid scratch_dir");

    let monotree_dir = args
        .scratch_dir
        .canonicalize()
        .expect("invalid scratch_dir")
        .join(
            args.github_repo
                .iter()
                .map(|s| format!("{}_{}", s.owner, s.repo))
                .collect::<Vec<_>>()
                .join("_"),
        )
        .join("persist");
    let temp_dir = monotree_dir.parent().unwrap().join("temp");
    std::fs::create_dir_all(&temp_dir).expect("invalid temp_dir");

    init_git(&monotree_dir, &args.github_repo);

    println!("Fetching diffs ...");
    util::chdir(&monotree_dir);
    util::check_call(util::git().args(["fetch", "--quiet", "--all"]));

    let mut base_names = Vec::new();
    let mut pull_blobs = Vec::new();
    for s in &args.github_repo {
        let util::Slug { owner, repo } = s;
        println!("Fetching open pulls for {sl} ...", sl = s.str());
        let base_name = github
            .repos(owner, repo)
            .get()
            .await?
            .default_branch
            .expect("remote api error");
        let pulls_api = github.pulls(owner, repo);
        let pulls = github
            .all_pages(
                pulls_api
                    .list()
                    .state(octocrab::params::State::Open)
                    .base(&base_name)
                    .send()
                    .await?,
            )
            .await?;
        println!(
            "Open {base_name}-pulls for {sl}: {len}",
            sl = s.str(),
            len = pulls.len()
        );
        base_names.push(base_name);
        pull_blobs.push((pulls, s));
    }
    let mut mono_pulls = Vec::new();
    for (ps, slug) in pull_blobs {
        let sl = slug.str();
        println!("Store diffs for {sl}");
        util::check_call(
            util::git()
                .args(["fetch", "--quiet"])
                .arg(format!("con_pull_ref/{sl}")),
        );
        for p in ps {
            let num = p.number;
            mono_pulls.push(MetaPull {
                pull: p,
                head_commit: util::check_output(
                    util::git()
                        .args(["log", "-1", "--format=%H"])
                        .arg(format!("upstream-pull/{num}/head")),
                ),
                slug: util::Slug {
                    owner: slug.owner.clone(),
                    repo: slug.repo.clone(),
                },
                slug_num: format!("{sl}/{num}"),
                merge_commit: None,
            })
        }
    }
    let base_name = base_names.first().expect("no repos given");
    util::check_call(
        util::git()
            .args(["fetch", "--quiet", "origin"])
            .arg(base_name),
    );

    {
        let temp_git_work_tree_ctx = tempfile::TempDir::new_in(&temp_dir).expect("tempdir error");
        let temp_git_work_tree = temp_git_work_tree_ctx.path();

        util::check_call(
            std::process::Command::new("cp")
                .arg("-r")
                .arg(monotree_dir.join(".git"))
                .arg(temp_git_work_tree.join(".git")),
        );

        util::chdir(temp_git_work_tree);
        println!("Calculate mergeable pulls");

        let mono_pulls_mergeable = calc_mergeable(mono_pulls, base_name);
        if args.update_comments {
            for (i, pull_update) in mono_pulls_mergeable.iter().enumerate() {
                println!(
                    "{i}/{len} Checking for conflicts {base_name} <> {pr_id} <> other_pulls ... ",
                    len = mono_pulls_mergeable.len(),
                    pr_id = pull_update.slug_num
                );
                let pulls_conflict = calc_conflicts(&mono_pulls_mergeable, pull_update);
                update_comment(&config, &github, args.dry_run, pull_update, &pulls_conflict)
                    .await?;
            }
        }
        if let Some(pull_id) = args.pull_id {
            let found = mono_pulls_mergeable.iter().find(|p| p.slug_num == pull_id);
            if found.is_none() {
                println!(
                    "{id} not found in all {len} open, mergeable {base_name} pulls",
                    id = pull_id,
                    len = mono_pulls_mergeable.len()
                );
                return Ok(());
            }
            let pull_merge = found.unwrap();
            println!(
                "Checking for conflicts {base_name} <> {id} <> other_pulls ... ",
                id = pull_merge.slug_num
            );
            let conflicts = calc_conflicts(&mono_pulls_mergeable, pull_merge);
            update_comment(&config, &github, args.dry_run, pull_merge, &conflicts).await?;
        }
    }
    util::chdir(&temp_dir);

    Ok(())
}
