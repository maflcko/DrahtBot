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

    /// The 'locale' folder that contains *.ts files
    #[arg(long)]
    translation_dir: String,

    #[arg(long)]
    cache_dir: String,
}

fn main() {
    let args = Args::parse();

    // Alternative LLMs for translations could be Mistral 3.1 or OpenAI 4.1-nano, or the "thinking"
    // ones Gemini-flash-2.5, openai-o4-mini, or R1.

    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemma-3-27b-it:generateContent?key={}",args.llm_api_key);
    let ts_dir = fs::canonicalize(args.translation_dir).expect("locale dir must exist");
    let cache_dir = fs::canonicalize(args.cache_dir).expect("cache dir must exist");

    for entry in fs::read_dir(ts_dir).expect("locale dir must exist") {
        let entry = entry.expect("locale file must exist");
        let name = entry
            .file_name()
            .into_string()
            .expect("file name must be utf8");

        if !name.ends_with(".ts") {
            println!("Skip file {name}");
            continue;
        }

        let lang = name
            .strip_prefix("bitcoin_")
            .expect("ts file name unexpected")
            .strip_suffix(".ts")
            .expect("ts file name unexpected");

        let ts = fs::read_to_string(entry.path()).expect("Unable to read translation file");

        check(lang, &cache_dir, &ts, &url);
    }
}

fn cache_key(msg: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(msg);
    let result = hasher.finalize();
    format!("cache_translation_check_{:x}", result)
}

fn print_result(cache_file: &Path, res: &str, prompt: &str) {
    if res.starts_with("NO") {
        // no spam, all good
    } else if res.starts_with("YES") || res.starts_with("UNK_LANG") {
        println!(
            "\n#### Erroneous translation:\n[cache file]: {file}\n{prompt}\n{res}\n---\n",
            file = cache_file
                .file_name()
                .expect("cache file must have name")
                .to_str()
                .expect("cache file name must be valid utf8"),
        );
    } else {
        panic!("File {} corrupt!\nAdjust prompt?", cache_file.display());
    }
}

fn check(lang: &str, cache_dir: &Path, ts: &str, url: &str) {
    // From https://ai.google.dev/gemini-api/docs/rate-limits#current-rate-limits
    let rate_limit_wait = Duration::from_secs(24 * 60 * 60) / 14400;

    for msg in ts.split("<message>").skip(1) {
        let msg = msg
            .split("</message>")
            .next()
            .expect("Must have closed message tag");
        let msg = msg
            // shorten msg in prompt
            .replace("<translation type=\"unfinished\">", "<translation>")
            // Skip &amp; in msg
            .replace("&amp;", "");
        let prompt = format!(
            r#"
Evaluate the provided translation from English to the language '{lang}' for unwanted content, erroneous content, or spam.

- Assess the translation for accuracy and whether it is problematic in any way.
- The English text is wrapped in <source></source>
- The '{lang}' text is wrapped in <translation></translation>
- Ensure that format specifiers (% prefix) are taken over correctly from the source to the translation.

# Output Format

- If the translation is unproblematic, output: "NO".
- If the translation is problematic, output: "YES", followed by a brief explanation and the correct translation.
- If you are unfamiliar with the language specified by '{lang}', output: "UNK_LANG".
- You must start your output with "NO", or "YES", or "UNK_LANG".

# Translation context

The translation appears in the context of Bitcoin:
- "address" could mean a P2P Bitcoin network address, or a Bitcoin wallet address.
- "change" usually refers to the change amount in a Bitcoin transaction.

# Translation

{msg}

"#,
        );

        let cache_file = cache_dir.join(cache_key(&prompt));

        match fs::read_to_string(&cache_file) {
            Ok(contents) => {
                print_result(&cache_file, &contents, &prompt);
            }
            Err(_) => {
                println!("Cache miss for prompt=\n{prompt}");
                let sleep_target = Instant::now() + rate_limit_wait;
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
                fs::write(&cache_file, val).expect("Must be able to write cache file");
                print_result(&cache_file, val, &prompt);
                sleep(sleep_target - Instant::now());
            }
        }
    }
}
