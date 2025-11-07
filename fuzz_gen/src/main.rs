use clap::Parser;
use std::process::Command;
use util::{chdir, check_call, git};

#[derive(clap::Parser)]
#[command(long_about = format!(r#"

Generate Bitcoin Core fuzz inputs until a crash.

To prepare, install:
wget cargo sed git python3 ccache screen + Bitcoin Core deps
#
# https://apt.llvm.org/
#
# wget https://apt.llvm.org/llvm.sh && chmod +x llvm.sh && ./llvm.sh {}
#
"#, LLVM_VER))]
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

const LLVM_VER: &str = "22";
const FUZZ_CORPORA_PATH_ELEMENT: &str = "fuzz_corpora";

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
    check_call(Command::new("wget").arg(
        "https://github.com/bitcoin/bitcoin/commit/9999b602983887002ff5d06bcd593ad91b81639c.diff",
    ));
    check_call(git().args(["apply", "9999b602983887002ff5d06bcd593ad91b81639c.diff"]));
    for replacement in [
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
    check_call(Command::new("cmake").args([
        "-B",
        "./bld",
        "-DBUILD_FOR_FUZZING=ON",
        &format!("-DCMAKE_C_COMPILER=clang-{}", LLVM_VER),
        &format!(
            "-DCMAKE_CXX_COMPILER=clang++-{};-D_GLIBCXX_ASSERTIONS",
            LLVM_VER
        ),
        &format!("-DSANITIZERS={}", args.sanitizers),
    ]));
    check_call(Command::new("cmake").args([
        "--build",
        "./bld",
        &format!("--parallel={}", args.jobs),
    ]));
    check_call(Command::new("rm").arg("-rf").arg(&dir_generate_seeds));
    let fuzz = || {
        let mut cmd = Command::new("python3");
        cmd.env(
            "LLVM_SYMBOLIZER_PATH",
            format!("/usr/bin/llvm-symbolizer-{}", LLVM_VER),
        )
        .args([
            "./bld/test/fuzz/test_runner.py",
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
            .arg(dir_assets.join(FUZZ_CORPORA_PATH_ELEMENT)),
    );
    check_call(fuzz().arg(&dir_generate_seeds).arg("--generate"));
    check_call(
        fuzz()
            .arg(dir_assets.join(FUZZ_CORPORA_PATH_ELEMENT))
            .arg("--m_dir")
            .arg(&dir_generate_seeds),
    );
}
