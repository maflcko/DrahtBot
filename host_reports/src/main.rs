use clap::Parser;

#[derive(clap::Parser)]
#[command(about = "Pull a git repository and move it to /var/www/... .", long_about = None)]
struct Args {
    /// The repo slug of the remote on GitHub for reports.
    #[arg(long)]
    repo_report: util::Slug,
    /// The local scratch folder.
    #[arg(long)]
    host_reports_scratch: std::path::PathBuf,
    /// Print changes/edits, only modify the scratch folder.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

fn main() {
    let args = Args::parse();

    println!();
    println!("See guix script for instructions on how to add write permission for /var/www to the current user");
    println!();

    let repo_url = format!("https://github.com/{}", args.repo_report.str());
    let host_reports_www_folder = if args.dry_run {
        args.host_reports_scratch.join("www_output/")
    } else {
        std::path::Path::new("/var/www/html/host_reports/").join(args.repo_report.str())
    };

    if !host_reports_www_folder.is_dir() {
        println!(
            "Clone {repo_url} repo to {dir}",
            dir = host_reports_www_folder.display()
        );
        util::check_call(
            util::git()
                .args(["clone", "--quiet", &repo_url])
                .arg(&host_reports_www_folder),
        );
    }

    println!("Fetch upsteam, checkout latest `main` branch");
    util::chdir(&host_reports_www_folder);
    util::check_call(util::git().args(["fetch", "--quiet", "--all"]));
    util::check_call(util::git().args(["checkout", "origin/main"]));
    util::check_call(util::git().args(["reset", "--hard", "HEAD"]));
}
