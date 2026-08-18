use clap::Parser;
use std::fs;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::path::Path;
use std::process::Command;
use util::{LLM_SHARED_PROMPT_DIFF, LLM_TYPOS, make_llm_payload, prepare_raw_diff_for_llm};

#[derive(Parser)]
#[command(about = "Scratch script to evaluate LLMs.", long_about = None)]
struct Cli {
    #[arg(long)]
    open_ai_token: String,
    #[arg(long)]
    google_ai_token: String,
}

fn main() {
    let cli = Cli::parse();

    let inputs = fs::canonicalize("./inputs").expect("folder must exist");

    let outputs = format!(
        "./outputs-{id}",
        id = {
            let date = Command::new("date")
                .arg("--iso-8601=ns")
                .output()
                .expect("Failed to execute date command");
            assert!(date.status.success());
            String::from_utf8(date.stdout)
                .expect("must be utf8")
                .trim()
                .to_string()
        }
    );
    fs::create_dir(&outputs).expect("folder must be creatable");
    let outputs = fs::canonicalize(outputs).expect("folder must exist");

    for entry in fs::read_dir(inputs).expect("folder must exist") {
        let entry = entry.expect("file must exist");
        let file_name = entry
            .path()
            .file_name()
            .expect("file must have name")
            .to_str()
            .expect("Must be valid utf8")
            .to_string();
        let diff = fs::read_to_string(entry.path()).expect("Must be able to read diff");

        let diff = format!(
            "{}\n{}",
            // Inject seed to avoid cached input
            RandomState::new().build_hasher().finish(),
            prepare_raw_diff_for_llm(&diff)
        );

        //check_google_ai(&cli, &outputs, &file_name, &diff);
        check_open_ai(&cli, &outputs, &file_name, &diff);
    }
}

fn check_google_ai(cli: &Cli, outputs: &Path, file_name: &str, diff: &str) {
    println!("Check {file_name} via google_ai");
    let payload = serde_json::json!({
      "systemInstruction": {
         "parts": [
           {
               "text":LLM_SHARED_PROMPT_DIFF
           },
         ]
       },
       "contents": [
        {
          "parts": [
            {
              "text": diff
            }
          ]
        },
        {
          "parts": [
            {
              "text": LLM_TYPOS.prompt()
            }
          ]
        }
      ]
    });
    let temp = outputs
        .join("temp_scratch")
        .to_str()
        .expect("must be valid utf8")
        .to_string();
    fs::write(
        &temp,
        serde_json::to_string(&payload).expect("must be valid json"),
    )
    .expect("Must be able to write file");
    let curl_out = Command::new("curl")
        .arg(format!(
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-lite-preview-06-17:generateContent?key={}"
            ,cli.google_ai_token))
        .arg("--fail")
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("-X")
        .arg("POST")
        .arg("-d")
        .arg(format!("@{}", temp))
        .output()
        .expect("curl error");
    assert!(curl_out.status.success());
    let response: serde_json::Value =
        serde_json::from_str(&String::from_utf8(curl_out.stdout).expect("must be valid utf8"))
            .expect("must be valid json");
    let val = response["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .expect("Content not found");
    if val.is_empty() {
        // Could be due to https://discuss.ai.google.dev/t/gemini-2-5-pro-with-empty-response-text/81175/23 or just hitting the output token limit
        println!("EMPTY:\n{response}");
    }
    fs::write(
        outputs.join(format!("{}.google_ai.dbg", file_name)),
        format!("{response}"),
    )
    .expect("Must be able to write file");
    fs::write(outputs.join(format!("{}.google_ai.txt", file_name)), val)
        .expect("Must be able to write file");
}

fn check_open_ai(cli: &Cli, outputs: &Path, file_name: &str, diff: &str) {
    println!("Check {file_name} via open_ai");
    let payload = make_llm_payload(diff, LLM_TYPOS.prompt());
    let temp = outputs
        .join("temp_scratch")
        .to_str()
        .expect("must be valid utf8")
        .to_string();
    fs::write(
        &temp,
        serde_json::to_string(&payload).expect("must be valid json"),
    )
    .expect("Must be able to write file");
    let curl_out = Command::new("curl")
        .arg("--fail")
        .arg("-X")
        .arg("POST")
        .arg("https://api.openai.com/v1/chat/completions")
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("-H")
        .arg(format!("Authorization: Bearer {}", cli.open_ai_token))
        .arg("-d")
        .arg(format!("@{}", temp))
        .output()
        .expect("curl error");
    assert!(curl_out.status.success());
    let response: serde_json::Value =
        serde_json::from_str(&String::from_utf8(curl_out.stdout).expect("must be valid utf8"))
            .expect("must be valid json");
    let val = response["choices"][0]["message"]["content"]
        .as_str()
        .expect("Content not found");
    fs::write(outputs.join(format!("{}.open_ai.txt", file_name)), val)
        .expect("Must be able to write file");
}
