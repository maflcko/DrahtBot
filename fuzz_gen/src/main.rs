use clap::Parser;
use std::process::Command;
use util::{chdir, check_call, git};

#[derive(clap::Parser)]
#[command(long_about = r#"

Generate fuzz inputs until a crash.

To prepare, install:
wget cargo sed git python3 ccache screen + Bitcoin Core deps
#
# https://apt.llvm.org/
#
# wget https://apt.llvm.org/llvm.sh
# chmod +x llvm.sh
# ./llvm.sh 19
#
"#)]
struct Args {
    /// The local scratch folder.
    #[arg(long)]
    scratch_folder: std::path::PathBuf,
    /// The number of jobs.
    #[arg(long, default_value_t = 1)]
    jobs: u8,
    /// The sanitizers to enable (must include fuzzer)
    #[arg(
        long,
        default_value = "address,fuzzer,undefined,integer,float-divide-by-zero"
    )]
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

    let url_code = format!("https://github.com/{}", "bitcoin/bitcoin");
    let url_seed = format!("https://github.com/{}", "bitcoin-core/qa-assets");
    std::fs::create_dir_all(&args.scratch_folder).expect("Failed to create scratch folder");
    let temp_dir = args
        .scratch_folder
        .canonicalize()
        .expect("Failed to canonicalize scratch dir folder");
    let dir_code = temp_dir.join("code");
    let dir_assets = temp_dir.join("assets");
    let dir_generate_seeds = temp_dir.join("fuzz_inputs_generate");

    ensure_init_git(&dir_code, &url_code);
    ensure_init_git(&dir_assets, &url_seed);

    println!("Fetch upsteam, checkout latest branch");
    chdir(&dir_code);
    check_call(git().args(["fetch", "--quiet", "--all"]));
    check_call(git().args(["checkout", "origin/master", "--force"]));
    check_call(git().args(["reset", "--hard", "HEAD"]));
    check_call(git().args(["clean", "-dfx"]));
    for replacement in [
        r#"s/llvm-symbolizer"/llvm-symbolizer-19"/g"#,
        r#"s/set_cover_merge=1/merge=1/g"#,
        r#"s/use_value_profile=0/use_value_profile=1/g"#,
    ] {
        check_call(Command::new("sed").args(["-i", replacement, "test/fuzz/test_runner.py"]));
    }

    chdir(&dir_assets);
    check_call(git().args(["fetch", "--quiet", "--all"]));
    check_call(git().args(["add", "--all"]));
    check_call(git().args(["commit", "--allow-empty", "-m", "Add inputs"]));
    check_call(git().args(["merge", "--no-edit", "origin/main"]));

    chdir(&dir_code);
    check_call(&mut Command::new("./autogen.sh"));
    check_call(
        Command::new("./configure")
            .args(["CC=clang-19", "CXX=clang++-19", "--enable-fuzz"])
            .arg(format!("--with-sanitizers={}", args.sanitizers)),
    );
    check_call(Command::new("make").arg("clean"));
    check_call(Command::new("make").arg(format!("-j{}", args.jobs)));
    check_call(Command::new("rm").arg("-rf").arg(&dir_generate_seeds));
    let fuzz = || {
        let mut cmd = Command::new("python3");
        cmd.args([
            "test/fuzz/test_runner.py",
            "-l=DEBUG",
            //"--exclude=coinselection",
        ])
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
