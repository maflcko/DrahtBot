use clap::Parser;
use util::{chdir, check_call, check_output, git};

fn gen_coverage(
    docker_exec: &dyn Fn(&str),
    assets_dir: &std::path::Path,
    dir_code: &std::path::Path,
    dir_result: &std::path::Path,
    git_ref: &str,
    make_jobs: u8,
) {
    println!(
        "Generate coverage for {} in {} (ref: {}).",
        dir_code.display(),
        dir_result.display(),
        git_ref
    );
    chdir(dir_code);
    let dir_build = dir_code.join("build");

    println!("Clear previous build and result folders");

    let clear_dir = |folder: &std::path::Path| {
        std::fs::create_dir_all(folder).expect("Failed to create a folder");
        docker_exec(&format!("rm -r {}", folder.display()));
        std::fs::create_dir_all(folder).expect("Failed to create a folder");
        // Must change to a dir that exists after this function call
    };

    clear_dir(&dir_build);
    clear_dir(dir_result);

    println!("Make coverage data in docker ...");
    chdir(dir_code);

    docker_exec(&format!(
        "cmake -B {} -DBUILD_FOR_FUZZING=ON \
         -DAPPEND_CXXFLAGS='-fprofile-update=atomic' \
         -DAPPEND_CFLAGS='-fprofile-update=atomic' \
         -DCMAKE_BUILD_TYPE=Coverage",
        dir_build.display()
    ));
    docker_exec(&format!(
        "cmake --build {} -j{}",
        dir_build.display(),
        make_jobs
    ));

    println!("Make coverage ...");
    docker_exec(&format!(
        "cmake -DJOBS={} -DFUZZ_CORPORA_DIR={}/fuzz_corpora \
         -DLCOV_OPTS='--rc branch_coverage=1 --ignore-errors mismatch,mismatch,inconsistent,inconsistent' \
         -P {}/CoverageFuzz.cmake",
        make_jobs,
        assets_dir.display(),
        dir_build.display()
    ));
    docker_exec(&format!(
        "mv {}/*coverage* {}/",
        dir_build.display(),
        dir_result.display()
    ));
    chdir(dir_result);
    check_call(git().args(["checkout", "main"]));
    check_call(git().args(["add", "./"]));
    check_call(git().args([
        "commit",
        "-m",
        &format!("Add fuzz coverage results for {}", git_ref),
    ]));
    check_call(git().args(["push", "origin", "main"]));

    // Work around permission errors
    clear_dir(dir_result);
    chdir(dir_result);
    check_call(git().args(["reset", "--hard", "HEAD"]));
}

fn calc_coverage(
    assets_dir: &std::path::Path,
    dir_code: &std::path::Path,
    dir_cov_report: &std::path::Path,
    make_jobs: u8,
    remote_url: &str,
) {
    println!("Start docker process ...");
    std::fs::create_dir_all(dir_cov_report).expect("Failed to create dir_cov_report");
    let docker_id = check_output(std::process::Command::new("podman").args([
        "run",
        "-idt",
        "--rm",
        &format!(
            "--volume={}:{}:rw,z",
            assets_dir.display(),
            assets_dir.display()
        ),
        &format!(
            "--volume={}:{}:rw,z",
            dir_code.display(),
            dir_code.display()
        ),
        &format!(
            "--volume={}:{}:rw,z",
            dir_cov_report.display(),
            dir_cov_report.display()
        ),
        //'--mount', # Doesn't work with fedora (needs rw,z)
        //'type=bind,src={},dst={}'.format(dir_code, dir_code),
        //'--mount',
        //'type=bind,src={},dst={}'.format(dir_cov_report, dir_cov_report),
        "-e",
        "LC_ALL=C.UTF-8",
        "ubuntu:devel",
    ]));

    let docker_exec = |cmd: &str| {
        check_call(std::process::Command::new("podman").args([
            "exec",
            &docker_id,
            "bash",
            "-c",
            &format!(
                "cd {} && {}",
                std::env::current_dir().expect("Failed to getcwd").display(),
                cmd
            ),
        ]))
    };

    println!("Docker running with id {}.", docker_id);

    println!("Installing packages ...");
    docker_exec("apt-get update");
    docker_exec(&format!("apt-get install -qq {}", "python3-zmq libsqlite3-dev libevent-dev libboost-dev libdb5.3++-dev libzmq3-dev lcov build-essential cmake pkg-config"));

    println!("Generate coverage");
    chdir(dir_code);
    let base_git_ref = &check_output(git().args(["log", "--format=%H", "-1", "HEAD"]))[..16];
    chdir(assets_dir);
    let assets_git_ref = &check_output(git().args(["log", "--format=%H", "-1", "HEAD"]))[..16];
    let dir_result_base = dir_cov_report.join(base_git_ref).join(assets_git_ref);
    gen_coverage(
        &docker_exec,
        assets_dir,
        dir_code,
        &dir_result_base,
        &format!("{base_git_ref}-code {assets_git_ref}-assets"),
        make_jobs,
    );

    println!("{remote_url}/coverage_fuzz/monotree/{base_git_ref}/{assets_git_ref}/fuzz.coverage/index.html");
}

#[derive(clap::Parser)]
#[command(about = "Run fuzz coverage reports.", long_about = None)]
struct Args {
    /// The repo slug of the remote on GitHub for reports.
    #[arg(long, default_value = "DrahtBot/reports")]
    repo_report: util::Slug,
    /// The remote url of the hosted html reports.
    #[arg(
        long,
        default_value = "https://drahtbot.space/host_reports/DrahtBot/reports"
    )]
    remote_url: String,
    /// The number of make jobs.
    #[arg(long, default_value_t = 2)]
    make_jobs: u8,
    /// The local dir used for scratching.
    #[arg(long)]
    scratch_dir: std::path::PathBuf,
    /// The ssh key for "repo_report".
    #[arg(long)]
    ssh_key: std::path::PathBuf,
    /// Which git ref in the code repo to build.
    #[arg(long, default_value = "master")]
    git_ref_code: String,
    /// Which git ref in the qa-assets repo to use.
    #[arg(long, default_value = "main")]
    git_ref_qa_assets: String,
    /// Which targets to build.
    #[arg(long, default_value = "")]
    fuzz_targets: String,
}

fn ensure_init_git(folder: &std::path::Path, url: &str) {
    println!("Clone {url} repo to {dir}", dir = folder.display());
    if !folder.is_dir() {
        check_call(git().args(["clone", "--quiet", url]).arg(folder));
    }
}

fn main() {
    let args = Args::parse();

    std::fs::create_dir_all(&args.scratch_dir).expect("Failed to create scratch folder");
    let temp_dir = args
        .scratch_dir
        .canonicalize()
        .expect("Failed to canonicalize scratch folder");
    let ssh_cmd = format!(
        "ssh -i {} -F /dev/null",
        args.ssh_key
            .canonicalize()
            .expect("Failed to canonicalize ssh key")
            .display()
    );

    let code_dir = temp_dir.join("code").join("monotree");
    let code_url = "https://github.com/bitcoin/bitcoin";
    let report_dir = temp_dir.join("reports");
    let report_url = format!("git@github.com:{}.git", args.repo_report.str());
    let assets_dir = temp_dir.join("assets");
    let assets_url = "https://github.com/bitcoin-core/qa-assets";

    ensure_init_git(&code_dir, code_url);
    ensure_init_git(&report_dir, &report_url);
    ensure_init_git(&assets_dir, assets_url);

    println!("Set git metadata");
    chdir(&report_dir);
    check_call(git().args([
        "config",
        "user.email",
        "39886733+DrahtBot@users.noreply.github.com",
    ]));
    check_call(git().args(["config", "user.name", "DrahtBot"]));
    check_call(git().args(["config", "core.sshCommand", &ssh_cmd]));

    println!("Fetching diffs ...");
    chdir(&code_dir);
    check_call(git().args(["fetch", "origin", "--quiet", &args.git_ref_code]));
    check_call(git().args(["checkout", "FETCH_HEAD", "--force"]));
    check_call(git().args(["reset", "--hard", "HEAD"]));
    check_call(git().args(["clean", "-dfx"]));
    check_call(std::process::Command::new("sed").args([
        "-i",
        &format!(
            "s/FUZZ_CORPORA_DIR}}/FUZZ_CORPORA_DIR}} {} /g",
            args.fuzz_targets
        ),
        "cmake/script/CoverageFuzz.cmake",
    ]));
    chdir(&report_dir);
    check_call(git().args(["fetch", "--quiet", "--all"]));
    check_call(git().args(["reset", "--hard", "HEAD"]));
    check_call(git().args(["checkout", "main"]));
    check_call(git().args(["reset", "--hard", "origin/main"]));
    chdir(&assets_dir);
    check_call(git().args(["fetch", "origin", "--quiet", &args.git_ref_qa_assets]));
    check_call(git().args(["checkout", "FETCH_HEAD", "--force"]));
    check_call(git().args(["clean", "-dfx"]));

    calc_coverage(
        &assets_dir,
        &code_dir,
        &report_dir.join("coverage_fuzz").join("monotree"),
        args.make_jobs,
        &args.remote_url,
    );
}
