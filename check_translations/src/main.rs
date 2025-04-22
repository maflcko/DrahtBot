use clap::Parser;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Args {
    /// From https://aistudio.google.com/apikey
    #[arg(long)]
    llm_api_key: String,

    #[arg(long)]
    translation_file: String,

    #[arg(long)]
    cache_dir: String,
}

fn main() {
    let args = Args::parse();

    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemma-3-27b-it:generateContent?key={}",args.llm_api_key);
    let ts_path = fs::canonicalize(args.translation_file).expect("translation file must exist");
    let cache_dir = fs::canonicalize(args.cache_dir).expect("cache dir must exist");

    let ts = fs::read_to_string(ts_path).expect("Unable to read translation file");

    check(&cache_dir, &ts, &url);
}

fn cache_key(msg: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(msg);
    let result = hasher.finalize();
    format!("cache_translation_check_{:x}", result)
}

fn print_result(file: &Path, res: &str, msg: &str) {
    match res {
        "YES" => {
            println!("Erroneous translation:\n{}", msg);
        }
        "NO" => {
            // no spam, all good
        }
        _ => {
            panic!("File {} corrupt!\nAdjust prompt?", file.display());
        }
    }
}

fn check(cache_dir: &Path, ts: &str, url: &str) {
    // From https://ai.google.dev/gemini-api/docs/rate-limits#current-rate-limits
    let rate_limit_wait = Duration::from_secs(24 * 60 * 60) / 14400;

    for msg in ts.split("<message>").skip(1) {
        let msg = msg
            .split("</message>")
            .next()
            .expect("Must have closed message tag");

        let cache_file = cache_dir.join(cache_key(msg));

        match fs::read_to_string(&cache_file) {
            Ok(contents) => {
                print_result(&cache_file, &contents, msg);
            }
            Err(_) => {
                println!("Cache miss for msg=\n{}", msg);
                let sleep_target = Instant::now() + rate_limit_wait;
                let prompt = format!(
                    r#"
Does the following translation contain unwanted content or spam? Reply either with "YES" or "NO".

{}
        "#,
                    msg
                );
                let payload = json!({
                    "contents": [
                        {
                            "role": "user",
                            "parts": [
                                {
                                    "text": prompt
                                }
                            ]
                        }
                    ]
                });

                let curl_out = Command::new("curl")
                    .arg("-X")
                    .arg("POST")
                    .arg("-H")
                    .arg("Content-Type: application/json")
                    .arg(url)
                    .arg("-d")
                    .arg(serde_json::to_string(&payload).expect("Failed to serialize payload"))
                    .stderr(Stdio::inherit())
                    .output()
                    .expect("Failed to execute curl");
                assert!(curl_out.status.success());
                let response: serde_json::Value = serde_json::from_str(
                    &String::from_utf8(curl_out.stdout).expect("must be valid utf8"),
                )
                .expect("must be valid json");
                println!("... {response}");
                let val = response["candidates"][0]["content"]["parts"][0]["text"]
                    .as_str()
                    .expect("Content not found")
                    .trim();
                assert!(val == "YES" || val == "NO"); // Adjust prompt on failure?
                fs::write(&cache_file, val).expect("Must be able to write cache file");
                print_result(&cache_file, val, msg);
                sleep(sleep_target - Instant::now());
            }
        }
    }
}
