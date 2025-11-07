use clap::Parser;

#[derive(clap::Parser)]
#[command(about = "Fetch Bitcoin Core depends sources and move them to /var/www/.", long_about = None)]
struct Args {
    /// The repo slug of the remote on GitHub. Format: owner/repo
    #[arg(long)]
    github_repo: util::Slug,
    /// The git ref to checkout and fetch the depends from.
    #[arg(long, default_value = "origin/master")]
    git_ref: String,
    /// The local dir used for scratching.
    #[arg(long)]
    scratch_dir: std::path::PathBuf,
    /// Print changes/edits instead of moving the files.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

fn main() -> Result<(), std::io::Error> {
    let args = Args::parse();

    println!();
    println!("Same setup as the guix builds.");
    println!();

    let git_remote_url = format!("https://github.com/{}", args.github_repo.str());
    let www_folder_depends_caches =
        std::path::Path::new("/var/www/html/depends_download_fallback/");
    std::fs::create_dir_all(&args.scratch_dir).expect("invalid scratch_dir");
    let git_repo_dir = args
        .scratch_dir
        .canonicalize()
        .expect("invalid scratch_dir")
        .join("git_repo");
    let temp_dir = git_repo_dir.parent().unwrap();

    if !args.dry_run {
        println!(
            "Create folder {} if it does not exist",
            www_folder_depends_caches.display()
        );
        std::fs::create_dir_all(www_folder_depends_caches)?;
    }
    if !git_repo_dir.is_dir() {
        println!(
            "Clone {} repo to {}",
            git_remote_url,
            git_repo_dir.display()
        );
        util::chdir(temp_dir);
        util::check_call(
            util::git()
                .args(["clone", "--quiet", &git_remote_url])
                .arg(&git_repo_dir),
        );
    }

    println!("Fetch upsteam, checkout {}", args.git_ref);
    util::chdir(&git_repo_dir);
    util::check_call(util::git().args(["fetch", "--quiet", "--all"]));
    util::check_call(util::git().args(["checkout", &args.git_ref]));

    println!("Download dependencies ...");
    util::chdir(&git_repo_dir.join("depends"));
    std::env::set_var("MULTIPROCESS", "1");
    util::check_call(std::process::Command::new("make").arg("download"));
    let source_dir = git_repo_dir.join("depends").join("sources");
    println!(
        "Merging results of {} to {}",
        source_dir.display(),
        www_folder_depends_caches.display()
    );
    for entry in std::fs::read_dir(source_dir)? {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }
        println!(" ... entry = {}", entry.file_name().to_string_lossy());
        if !args.dry_run {
            std::fs::copy(
                entry.path(),
                www_folder_depends_caches.join(entry.file_name()),
            )?;
        }
    }
    Ok(())
}
