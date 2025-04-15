use clap::Parser;
use std::collections::{HashMap, HashSet};
use std::env::{consts::ARCH, current_dir};
use std::fs;
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use util::{Slug, chdir, check_call, check_output, get_octocrab, get_pull_mergeable, git};

#[derive(clap::Parser)]
#[command(about="Guix build and create an issue comment to share the results.",long_about=None)]
struct Args {
    /// The access token for GitHub.
    #[arg(long)]
    github_access_token: Option<String>,
    /// The repo slugs of the remotes on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: Vec<Slug>,
    /// The local dir used for scratching.
    #[arg(long)]
    scratch_dir: PathBuf,
    /// The number of jobs
    #[arg(long, default_value_t = 2)]
    guix_jobs: u8,
    /// Where the assets are reachable
    #[arg(long, default_value = "http://127.0.0.1")]
    domain: String,
    /// Print changes/edits instead of calling the GitHub API.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    /// Only build this one commit and exit.
    #[arg(long)]
    build_one_commit: Option<String>,
}

const ID_GUIX_COMMENT: &str = "<!--9cd9c72976c961c55c7acef8f6ba82cd-->";
// Only update this after the change is merged to the main development branch of --github_repo:
//
// curl -LO "https://bitcoincore.org/depends-sources/sdks/Xcode-15.0-15A240d-extracted-SDK-with-libcxx-headers.tar.gz"
// mv                                                   ./Xcode-15.0-15A240d-extracted-SDK-with-libcxx-headers.tar.gz ./guix/
// cp                                              ./guix/Xcode-15.0-15A240d-extracted-SDK-with-libcxx-headers.tar.gz ./scratch/guix/
const CURRENT_XCODE_FILENAME: &str = "Xcode-15.0-15A240d-extracted-SDK-with-libcxx-headers.tar.gz";

fn lsdir<T: FromIterator<String>>(folder: &Path) -> T {
    fs::read_dir(folder)
        .expect("folder must exist to lsdir")
        .map(|entry| {
            entry
                .expect("folder entry must exist to lsdir")
                .file_name()
                .into_string()
                .expect("Must be valid file name")
        })
        .collect()
}

fn calculate_table(
    base_folder: &Path,
    commit_folder: &Path,
    external_url: &str,
    base_commit: &str,
    commit: &str,
) -> String {
    let mut rows: HashMap<String, [String; 2]> = HashMap::new(); // map from abbrev file name to list of links
    let shorten = |f: &str| {
        const PREFIX: &str = "bitcoin-";
        if f.starts_with(PREFIX) {
            format!("*{}", &f[(PREFIX.len() + 12)..])
        } else {
            f.to_string()
        }
    };
    let mut set_rows = |dir: &Path, row_id: usize, commit_sha: &str| {
        let mut files = lsdir::<Vec<_>>(dir);
        files.sort();
        for f in &files {
            let short_file_name = shorten(f);
            chdir(dir);
            rows.entry(short_file_name).or_default()[row_id] = format!(
                "[`{}...`]({}{}/{})",
                &check_output(Command::new("sha256sum").arg(f))[..16],
                external_url,
                commit_sha,
                f
            );
        }
    };
    set_rows(base_folder, 0, base_commit);
    set_rows(commit_folder, 1, commit);

    let mut text = String::new();
    for (f, [link1, link2]) in &rows {
        text += &format!("| {} | {} | {} |\n", f, link1, link2);
    }
    text += "\n";
    text
}

fn calculate_diffs(folder_1: &Path, folder_2: &Path) {
    let extensions = [".log"];
    let files: Vec<_> = lsdir::<HashSet<_>>(folder_1)
        .intersection(&lsdir::<HashSet<_>>(folder_2))
        .filter(|f| extensions.iter().any(|e| f.ends_with(e)))
        .cloned()
        .collect();
    for f in &files {
        let file_1 = folder_1.join(f);
        let file_2 = folder_2.join(f);
        let diff_file = folder_2.join(format!("{}.diff", f));

        check_call(Command::new("sh").arg("-c").arg(format!(
            "diff --color {} {} > {}",
            ensure_path_str(&file_1),
            ensure_path_str(&file_2),
            ensure_path_str(&diff_file)
        )));
    }
}

pub fn ensure_init_git(monotree: &Path, url: &str) {
    println!("Clone {url} repo to {dir}", dir = monotree.display());
    if !monotree.is_dir() {
        check_call(git().args(["clone", "--quiet", url]).arg(monotree));
    }
}

pub fn ensure_create_all(dir: PathBuf) -> PathBuf {
    fs::create_dir_all(&dir).expect("must be valid dir");
    dir
}

pub fn ensure_path_str(dir: &Path) -> &str {
    dir.to_str().expect("must be valid utf8 path")
}

pub fn github_url(owner: &str, repo: &str) -> String {
    format!("https://github.com/{}/{}", owner, repo)
}

#[tokio::main]
async fn main() -> octocrab::Result<()> {
    let args = Args::parse();

    println!();
    println!("rm /var/www/html/index.html");
    println!("sudo usermod -aG www-data $USER");
    println!("sudo chown -R www-data:www-data /var/www");
    println!("sudo chmod -R g+rw /var/www");
    println!("# Then reboot");
    println!();

    fs::create_dir_all(&args.scratch_dir).expect("invalid scratch_dir");
    let temp_dir = args
        .scratch_dir
        .canonicalize()
        .expect("invalid scratch_dir");
    chdir(&temp_dir);

    let url_element_repo = "monotree";
    let guix_www_folder = ensure_create_all(if args.dry_run {
        temp_dir.join("www_output")
    } else {
        format!("/var/www/html/guix/{}/", url_element_repo).into()
    });
    let external_url = format!("{}/guix/{}/", args.domain, url_element_repo);

    let git_repo_dir = temp_dir.join("git_monotree");
    for Slug { owner, repo } in &args.github_repo {
        let url = github_url(owner, repo);
        ensure_init_git(&git_repo_dir, &url);
    }

    let depends_sources_dir = ensure_create_all(temp_dir.join("depends_sources"));
    let depends_cache_dir = ensure_create_all(temp_dir.join("depends_cache"));
    let guix_store_dir = ensure_create_all(temp_dir.join("root_store"));
    let guix_bin_dir = ensure_create_all(temp_dir.join("root_bin"));

    let temp_dir_str = ensure_path_str(&temp_dir);
    let guix_www_folder_str = ensure_path_str(&guix_www_folder);
    let git_repo_dir_str = ensure_path_str(&git_repo_dir);
    let depends_sources_dir_str = ensure_path_str(&depends_sources_dir);
    let depends_cache_dir_str = ensure_path_str(&depends_cache_dir);
    let guix_store_dir_str = ensure_path_str(&guix_store_dir);
    let guix_bin_dir_str = ensure_path_str(&guix_bin_dir);

    if !args.dry_run {
        println!("Clean guix folder of old files");
        check_call(Command::new("sh").arg("-c").arg(format!(
            "find {} -mindepth 1 -maxdepth 1 -type d -ctime +15 | xargs rm -rf",
            guix_www_folder_str
        )));
    }

    println!("Start docker process ...");
    let docker_id = check_output(Command::new("docker").args([
        "run",
        "-idt",
        "--rm",
        "--privileged", // https://github.com/bitcoin/bitcoin/pull/17595#issuecomment-606407804
        &format!("--volume={guix_store_dir_str}:{}:rw,z", "/gnu"),
        &format!("--volume={guix_bin_dir_str}:{}:rw,z", "/var/guix"),
        &format!("--volume={temp_dir_str}:{temp_dir_str}:rw,z",),
        // '--mount', # Doesn't work with fedora (needs rw,z)
        // 'type=bind,src={},dst={}'.format(dir_code, dir_code),
        // '-e',
        // 'LC_ALL=C.UTF-8',
        "ubuntu:noble",
    ]));
    println!("Docker running with id {}.", docker_id);

    // Could be an AtomicU8, even with named and enumerated symbols
    let docker_bash_prefix = AtomicBool::new(true);
    let docker_exec_ret_code = |cmd: &str| {
        Command::new("docker")
            .args([
                "exec",
                &docker_id,
                "bash",
                "-c",
                &format!(
                    "export {env1} && export {env2} && {cmd1} && cd {pwd} && {cmd}",
                    env1 = "FORCE_DIRTY_WORKTREE=1",
                    env2 = "TMPDIR=/guix_temp_dir/",
                    cmd1 = match docker_bash_prefix.load(Ordering::SeqCst) {
                        true => "true",
                        false => "source /config_guix/current/etc/profile",
                    },
                    pwd = ensure_path_str(&current_dir().expect("must have PWD")),
                    cmd = cmd
                ),
            ])
            .status()
            .expect("Failed to execute command")
    };
    let docker_exec = |cmd: &str| {
        assert!(docker_exec_ret_code(cmd).success());
    };

    docker_exec("mkdir /guix_temp_dir/");

    println!("Installing packages ...");
    docker_exec("apt-get update");
    docker_exec(&format!(
        "apt-get install -qq {}",
        "netbase xz-utils git make curl"
    ));

    if fs::read_dir(guix_store_dir)
        .expect("dir must exist")
        .next()
        .is_none()
    {
        println!("Install guix");
        let guix_tar = format!("guix-binary-1.4.0.{ARCH}-linux.tar.xz");
        docker_exec(&format!("curl -LO https://ftp.gnu.org/gnu/guix/{guix_tar}"));
        docker_exec(&format!(
            "echo '{}  ./{}' | sha256sum -c",
            "236ca7c9c5958b1f396c2924fcc5bc9d6fdebcb1b4cf3c7c6d46d4bf660ed9c9", guix_tar
        ));
        docker_exec(&format!("tar -xf ./{}", guix_tar));
        docker_exec("mv var/guix/* /var/guix && mv gnu/* /gnu/");
    }

    docker_exec("mkdir -p /config_guix/");
    docker_exec("ls -lh /config_guix/");
    docker_exec("ln -sf /var/guix/profiles/per-user/root/current-guix /config_guix/current");
    docker_bash_prefix.store(false, Ordering::SeqCst);
    docker_exec("groupadd --system guixbuild");
    docker_exec(
        "for i in {01..10}; do useradd -g guixbuild -G guixbuild -d /var/empty -s $(which nologin) -c \"Guix build user $i\" --system guixbuilder$i; done",
    );

    docker_exec("guix archive --authorize < /config_guix/current/share/guix/ci.guix.info.pub");

    let call_guix_build = |commit: &str| {
        println!("Starting guix build for {commit} ...");
        chdir(&git_repo_dir);
        docker_exec("chown -R root:root ./");
        docker_exec("git clean -dfx");
        docker_exec(&format!("git checkout --quiet --force {}", commit));
        let depends_compiler_hash =
            check_output(git().args(["rev-parse", &format!("{}:./contrib/guix", commit)]));
        let depends_cache_subdir = depends_cache_dir.join(depends_compiler_hash);
        docker_exec_ret_code(&format!(
            "cp -r {}/built {}/depends/",
            ensure_path_str(&depends_cache_subdir),
            git_repo_dir_str
        ));
        docker_exec(&format!("mkdir -p {}/depends/SDKs/", git_repo_dir_str));
        docker_exec(&format!(
            "tar -xf {}/{CURRENT_XCODE_FILENAME} --directory {}/depends/SDKs/",
            temp_dir_str, git_repo_dir_str
        ));
        docker_exec(
            "sed -i -e 's/DBUILD_BENCH=OFF/DBUILD_BENCH=ON/g' $( git grep -l BUILD_BENCH ./contrib/guix/ )",
        );
        docker_exec("sed -i '/ x86_64-w64-mingw32$/d' ./contrib/guix/guix-build"); // For now, until guix 1.5
        docker_exec_ret_code(&format!(
            "( guix-daemon --build-users-group=guixbuild & (export V=1 && export VERBOSE=1 && export MAX_JOBS={} && export SOURCES_PATH={} && ./contrib/guix/guix-build > {}/outerr 2>&1 ) && kill %1 )",
            args.guix_jobs, depends_sources_dir_str, git_repo_dir_str
        ));
        docker_exec(&format!("rm -rf {}/*", depends_cache_dir_str));
        fs::create_dir_all(&depends_cache_subdir).expect("must be valid dir");
        docker_exec(&format!(
            "mv {}/depends/built {}/built",
            git_repo_dir_str,
            ensure_path_str(&depends_cache_subdir)
        ));
        let output_dir = git_repo_dir.join("guix-build-output");
        let output_dir_str = ensure_path_str(&output_dir);
        docker_exec(&format!(
            "mv {}/guix-build-*/output {}",
            git_repo_dir_str, output_dir_str,
        ));
        docker_exec(&format!(
            "mv {}/outerr {}/guix_build.log",
            git_repo_dir_str, output_dir_str,
        ));
        docker_exec_ret_code(&format!(
            "for i in {output_dir_str}/* ; do mv $i/* {output_dir_str}/ ; done",
        ));
        docker_exec_ret_code(&format!(
            "for i in {}/* ; do rmdir $i ; done",
            output_dir_str
        ));
        output_dir
    };

    if let Some(commit) = args.build_one_commit.as_deref() {
        for Slug { owner, repo } in &args.github_repo {
            let url = github_url(owner, repo);
            if git()
                .args(["fetch", &url, commit])
                .status()
                .expect("git failed")
                .success()
            {
                println!("Starting guix build for one commit ({}) ...", commit);
                let output_dir = call_guix_build(commit);
                println!("See folder:\n{}", ensure_path_str(&output_dir));
                println!("Exit");
                return Ok(());
            }
        }
        panic!("commit not found in all repos");
    }

    let github = get_octocrab(args.github_access_token)?;

    println!("Checking github repos ...");
    for Slug { owner, repo } in &args.github_repo {
        let url = github_url(owner, repo);
        let issues_api = github.issues(owner, repo);
        let pulls_api = github.pulls(owner, repo);

        let label_needs_guix = "DrahtBot Guix build requested";
        let search_fmt = format!(
            "repo:{owner}/{repo} is:open is:pr label:\"{label}\" ",
            owner = owner,
            repo = repo,
            label = label_needs_guix,
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

        for item in &items {
            let pull = match get_pull_mergeable(&pulls_api, item.number).await? {
                None => {
                    println!("Ignore closed(?) pull ({url}/pulls/{})", item.number);
                    continue;
                }
                Some(p) => p,
            };
            if !pull
                .mergeable
                .expect("Must have attribute set after get_pull_mergeable")
            {
                println!("Ignore unmergeable pull ({})", pull.url);
                continue;
            }

            chdir(&git_repo_dir);
            docker_exec(&format!("git fetch {url} pull/${}/merge", pull.number));
            let commit = check_output(git().args(["rev-parse", "FETCH_HEAD"]));
            let base_commit = check_output(git().args(["rev-parse", "FETCH_HEAD^1"]));

            let overwrite_guix = |commit_id: &str, dst: PathBuf| {
                let src = call_guix_build(commit_id);
                println!(
                    "Moving {} to {} (overwriting)",
                    src.display(),
                    dst.display()
                );
                let _ = fs::remove_dir_all(&dst);
                fs::rename(src, &dst).expect("Could not mv folder");
                dst
            };
            let base_folder = overwrite_guix(&base_commit, guix_www_folder.join(&base_commit));
            let commit_folder = overwrite_guix(&commit, guix_www_folder.join(&commit));

            calculate_diffs(&base_folder, &commit_folder);
            let mut text = ID_GUIX_COMMENT.to_string();
            text += "\n";
            text += &format!(
                "### Guix builds (on {}) [untrusted test-only build, possibly unsafe, not for production use]\n\n",
                ARCH
            );
            text += "| File ";
            text += &format!("| commit {}<br>({}) ", &base_commit, pull.base.ref_field);
            text += &format!("| commit {}<br>(pull/${}/merge) ", &commit, pull.number);
            text += "|\n";
            text += "|--|--|--|\n";

            text += &calculate_table(
                &base_folder,
                &commit_folder,
                &external_url,
                &base_commit,
                &commit,
            );

            println!("{}\n    .remove_label({})", pull.url, label_needs_guix);
            println!("    .create_comment({})", text);

            if !args.dry_run {
                issues_api.create_comment(pull.number, text).await?;
                issues_api
                    .remove_label(pull.number, label_needs_guix)
                    .await?;
            }
        }
    }
    println!("Checked github repos ...");
    Ok(())
}
