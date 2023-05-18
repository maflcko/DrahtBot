use clap::Parser;
use std::process::Command;
use util::{chdir, check_call, git};

#[derive(clap::Parser)]
#[command(about = "Generate fuzz seeds until a crash.", long_about = None)]
struct Args {
    /// The local scratch folder.
    #[arg(long)]
    scratch_folder: std::path::PathBuf,
    /// The number of jobs.
    #[arg(long, default_value_t = 1)]
    jobs: u8,
    /// The sanitizers to enable (must include fuzzer)
    #[arg(long, default_value = "address,fuzzer,undefined,integer")]
    sanitizers: String,
}

pub fn ensure_init_git(folder: &std::path::Path, url: &str) {
    println!("Clone {url} repo to {dir}", dir = folder.display());
    if !folder.is_dir() {
        check_call(git().args(["clone", "--quiet", url]).arg(folder));
    }
    println!("Set git metadata");
    chdir(folder);
    check_call(git().args(["config", "user.email", "no@ne.nl"]));
    check_call(git().args(["config", "user.name", "none"]));
}

fn main() {
    let args = Args::parse();

    println!();
    println!("To prepare, install:");
    println!("sed git ccache + Bitcoin Core deps");
    println!("#");
    println!("# https://apt.llvm.org/");
    println!("#");
    println!("# wget https://apt.llvm.org/llvm.sh");
    println!("# chmod +x llvm.sh");
    println!("# ./llvm.sh 17");
    println!();

    let url_code = format!("https://github.com/{}", "bitcoin/bitcoin");
    let url_seed = format!("https://github.com/{}", "bitcoin-core/qa-assets");
    let dir_code = args.scratch_folder.join("code");
    let dir_assets = args.scratch_folder.join("assets");
    let dir_generate_seeds = args.scratch_folder.join("generate_seeds");

    ensure_init_git(&dir_code, &url_code);
    ensure_init_git(&dir_assets, &url_seed);

    println!("Fetch upsteam, checkout latest branch");
    chdir(&dir_code);
    check_call(git().args(["fetch", "--quiet", "--all"]));
    check_call(git().args(["checkout", "origin/master", "--force"]));
    check_call(git().args(["reset", "--hard", "HEAD"]));
    check_call(git().args(["clean", "-dfx"]));
    check_call(Command::new("sed").args([
        "-i",
        r#"s/runs=100000/use_value_profile=1","-entropic=1","-cross_over=1","-cross_over_uniform_dist=1","-rss_limit_mb=4000","-max_total_time=6000/g"#,
        "test/fuzz/test_runner.py",
    ]));

    chdir(&dir_assets);
    check_call(git().args(["fetch", "--quiet", "--all"]));
    check_call(git().args(["add", "--all"]));
    check_call(git().args(["commit", "--allow-empty", "-m", "Add inputs"]));
    check_call(git().args(["merge", "--no-edit", "origin/main"]));

    chdir(&dir_code);
    check_call(&mut Command::new("./autogen.sh"));
    check_call(
        Command::new("./configure")
            .args(["CC=clang-17", "CXX=clang++-17", "--enable-fuzz"])
            .arg(format!("--with-sanitizers={}", args.sanitizers)),
    );
    check_call(Command::new("make").arg("clean"));
    check_call(Command::new("make").arg(format!("-j{}", args.jobs)));
    check_call(Command::new("rm").arg("-rf").arg(&dir_generate_seeds));
    let fuzz = || {
        let mut cmd = Command::new("python3");
        cmd.args(["test/fuzz/test_runner.py", "-l=DEBUG"])
            .arg(format!("--par={}", args.jobs));
        cmd
    };
    check_call(
        fuzz()
            .arg(&dir_generate_seeds)
            .arg("--m_dir")
            .arg(dir_assets.join("fuzz_seed_corpus")),
    );
    check_call(fuzz().arg(&dir_generate_seeds).arg("--generate"));
    check_call(
        fuzz()
            .arg(dir_assets.join("fuzz_seed_corpus"))
            .arg("--m_dir")
            .arg(&dir_generate_seeds),
    );
}
