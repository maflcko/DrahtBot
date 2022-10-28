use clap::Parser;

#[derive(Clone)]
struct SlugTok {
    owner: String,
    repo: String,
    ci_token: String,
}

impl std::str::FromStr for SlugTok {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Format: a/b:c
        let err = "Wrong format, see --help.";
        let mut it = s.split(':');
        let mut it_slug = it.next().ok_or(err)?.split('/');
        let res = Self {
            owner: it_slug.next().ok_or(err)?.to_string(),
            repo: it_slug.next().ok_or(err)?.to_string(),
            ci_token: it.next().ok_or(err)?.to_string(),
        };
        if it.next().is_none() && it_slug.next().is_none() {
            return Ok(res);
        }
        Err(err)
    }
}

#[derive(clap::Parser)]
#[command(about = "Trigger Cirrus CI to re-run.", long_about = None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the remotes on GitHub. Format: owner/repo:cirrus_org_token
    #[arg(long)]
    github_repo: Vec<SlugTok>,
    /// The task names to re-run.
    #[arg(long)]
    task: Vec<String>,
    /// How many minutes to sleep between pulls.
    #[arg(long, default_value_t = 55)]
    sleep_min: u64,
    /// Print changes/edits instead of calling the GitHub/CI API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

fn rerun(task: &serde_json::Value, token: &String, dry_run: bool) {
    let fmt = "json format error";
    let t_id = task.get("id").expect(fmt).as_str().expect(fmt);
    let t_name = task.get("name").expect(fmt);
    let raw_data = format!(
        r#"
                        {{
                            "query":"mutation
                            {{
                               rerun(
                                 input: {{
                                   attachTerminal: false, clientMutationId: \"rerun-{t_id}\", taskId: \"{t_id}\"
                                 }}
                               ) {{
                                  newTask {{
                                    id
                                  }}
                               }}
                             }}"
                         }}
                     "#
    );
    println!("Re-run task {t_name} (id: {t_id})");
    if !dry_run {
        let out = std::process::Command::new("curl")
            .arg("https://api.cirrus-ci.com/graphql")
            .arg("-X")
            .arg("POST")
            .arg("-H")
            .arg(format!("Authorization: Bearer {token}"))
            .arg("--data-raw")
            .arg(raw_data)
            .output()
            .expect("curl error");
        println!("{}", String::from_utf8_lossy(&out.stdout));
        assert!(out.status.success());
    }
}
async fn get_pull_mergeable(
    api: &octocrab::pulls::PullRequestHandler<'_>,
    number: u64,
) -> octocrab::Result<Option<octocrab::models::pulls::PullRequest>> {
    // https://docs.github.com/en/rest/guides/getting-started-with-the-git-database-api#checking-mergeability-of-pull-requests
    loop {
        let pull = api.get(number).await?;
        if pull.state.as_ref().unwrap() != &octocrab::models::IssueState::Open {
            return Ok(None);
        }
        if pull.mergeable.is_none() {
            std::thread::sleep(std::time::Duration::from_secs(3));
            continue;
        }
        return Ok(Some(pull));
    }
}

#[tokio::main]
async fn main() -> octocrab::Result<()> {
    let cli = Args::parse();
    let github = {
        let build = octocrab::Octocrab::builder();
        match cli.github_access_token {
            Some(tok) => build.personal_token(tok),
            None => build,
        }
        .build()?
    };
    for SlugTok {
        owner,
        repo,
        ci_token,
    } in cli.github_repo
    {
        println!("Get open pulls for {}/{} ...", owner, repo);
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
            let pull = get_pull_mergeable(&pulls_api, pull.number).await?;
            let pull = match pull {
                None => {
                    continue;
                }
                Some(p) => p,
            };
            if !pull.mergeable.unwrap() {
                continue;
            }
            let pull_num = pull.number;
            let raw_data = format!(
                r#"
                    {{
                        "query":"query
                        {{
                            ownerRepository(platform: \"github\", owner: \"{owner}\", name: \"{repo}\") {{
                              viewerPermission
                              builds(last: 1, branch: \"pull/{pull_num}\") {{
                                edges {{
                                  node {{
                                    tasks {{
                                      id
                                      name
                                    }}
                                  }}
                                }}
                              }}
                            }}
                        }}"
                     }}
                "#
            );
            let output = std::process::Command::new("curl")
                .arg("https://api.cirrus-ci.com/graphql")
                .arg("-X")
                .arg("POST")
                .arg("--data-raw")
                .arg(raw_data)
                .output()
                .expect("curl error");
            assert!(output.status.success());
            let json_parsed = serde_json::from_slice::<serde_json::value::Value>(&output.stdout)
                .expect("json parse error");
            let fmt = "json format error";
            let tasks = json_parsed
                .get("data")
                .expect(fmt)
                .get("ownerRepository")
                .expect(fmt)
                .get("builds")
                .expect(fmt)
                .get("edges")
                .expect(fmt)
                .get(0)
                .expect(fmt)
                .get("node")
                .expect(fmt)
                .get("tasks")
                .expect(fmt)
                .as_array()
                .expect(fmt);
            for task_name in &cli.task {
                let found = tasks
                    .iter()
                    .filter(|t| {
                        t.get("name")
                            .expect(fmt)
                            .as_str()
                            .expect(fmt)
                            .contains(task_name)
                    })
                    .next();
                if found.is_some() {
                    rerun(found.unwrap(), &ci_token, cli.dry_run)
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(cli.sleep_min * 60));
        }
    }
    Ok(())
}
