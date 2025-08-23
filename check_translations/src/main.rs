use clap::Parser;
use serde_json::json;
use std::fs;
use std::io::Write;
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

    /// A scratch folder to put temporary files for caching results
    #[arg(long)]
    cache_dir: String,

    /// A folder to export the reports to (reports will be overwritten)
    #[arg(long)]
    report_folder: String,

    /// Limit to those language files, instead of iterating over all files
    #[arg(long)]
    lang: Vec<String>,

    /// How long to sleep between requests in seconds
    #[arg(long, default_value = "0")]
    rate_limit: u64,
}

fn main() {
    let args = Args::parse();

    let ts_dir = fs::canonicalize(args.translation_dir).expect("locale dir must exist");
    let cache_dir = fs::canonicalize(args.cache_dir).expect("cache dir must exist (can be empty)");
    let report_folder = fs::canonicalize(args.report_folder)
        .expect("report folder must exist (files in it will be overwritten)");

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

        if lang == "en" || !args.lang.is_empty() && args.lang.iter().all(|a_l| a_l != lang) {
            println!("Skip file {name}");
            continue;
        }

        let ts = fs::read_to_string(entry.path()).expect("Unable to read translation file");

        let mut report_file = fs::File::create(report_folder.join(format!("{lang}.md")))
            .expect("must be able to create empty report file");
        report_file
        .write_all("# Translations Review by LLM (✨ experimental)\n\nThe review quality depends on the LLM and the language. To report LLM shortcomings for a specific language, please file an issue. It may be possible to re-run with a stronger model.\n\n".as_bytes())
        .unwrap();

        check(
            lang,
            &cache_dir,
            &ts,
            &args.llm_api_key,
            &report_file,
            Duration::from_secs(args.rate_limit),
        );
    }
}

fn cache_key(lang: &str, msg: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(msg);
    let result = hasher.finalize();
    format!("cache_translation_check_{lang}_{result:x}")
}

fn print_result(
    num_issues: &mut u32,
    cache_file: &Path,
    res: &str,
    prompt: &str,
    msg: &str,
    mut report_file: &fs::File,
) {
    if res.starts_with("NO") {
        // no spam, all good
    } else if res.starts_with("SPAM") || res.starts_with("ERR") || res.starts_with("UNK_LANG") {
        *num_issues += 1;
        report_file
            .write_all(
                format!(
                    "\n<!-- {cache_key} -->\n```\n{msg}\n{res}\n```\n",
                    cache_key = cache_file.file_name().unwrap().to_str().unwrap(),
                    msg = msg.trim_matches('\n'),
                    res = res.trim_matches('\n'),
                )
                .as_bytes(),
            )
            .unwrap();
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

fn check(
    lang: &str,
    cache_dir: &Path,
    ts: &str,
    token: &str,
    mut report_file: &fs::File,
    rate_limit_wait: Duration,
) {
    // Alternative LLMs for translations could be Mistral 3.1 or OpenAI 4.1-nano, or the "thinking"
    // ones Gemini-flash-2.5, openai-o4-mini, or R1.
    // For now use a model that has no rate limits.
    // From https://ai.google.dev/gemini-api/docs/rate-limits#tier-1
    let url = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";
    let model = "gemini-2.5-flash-lite";

    let url = "https://api.openai.com/v1/chat/completions";
    let model = "gpt-4.1";

    report_file
        .write_all(format!("\n\n<details><summary>{lang}</summary>\n\n[If the result is outdated or of low quality, please file an issue to request and updated run for this language.](../../issues/new?title=%5B{lang}%5D%20request)\n\n").as_bytes())
        .unwrap();

    let mut num_issues = 0;

    for msg in ts.split("<message>").skip(1) {
        let msg = msg
            .split("</message>")
            .next()
            .expect("Must have closed message tag");
        let msg = msg
            // shorten msg in prompt
            .replace("<translation type=\"unfinished\">", "<translation>");
        let shortcut_key_prompt = if msg.contains("&amp;") {
            "- A single &amp; in the English text and the translation is usually used to indicate the shortcut key. Allow it to be placed anywhere, but ensure it exists exactly once."
        } else {
            ""
        };
        let prompt = format!(
            r#"
Evaluate the provided translation from English to the language '{lang}' for unwanted content, erroneous content, or spam.

- Assess the translation for accuracy and whether it is problematic in any way.
- The English text is wrapped in <source></source>
- The '{lang}' text is wrapped in <translation></translation>
- Ensure that format specifiers (% prefix) are taken over correctly from the source to the translation.
- Ensure that no whitespace format issues exist. For example, stray spacing or double space.
{shortcut_key_prompt}


# Output Format

- If the translation is unproblematic, output: "NO".
- If you are unfamiliar with the language specified by '{lang}', output: "UNK_LANG".
- If the translation is into a language completely unrelated to '{lang}', or contains unrelated gibberish, output: "SPAM", followed by a brief explanation and the correct translation.
- If the translation is problematic for other reasons, output: "ERR", followed by a brief explanation and the correct translation.
- You must start your output with "NO", "ERR", "SPAM", or "UNK_LANG".


# Translation context

The translation appears in the context of Bitcoin:
- "address" could mean a P2P Bitcoin network address, or a Bitcoin wallet address.
- "change" usually refers to the change amount in a Bitcoin transaction.


# Example (correct translation for 'zh_CN')

        <source>%1 will download and store a copy of the Bitcoin block chain.</source>
        <translation>%1 将会下载并存储比特币区块链。</translation>

<reply>NO</reply>


# Example (unknown language for 'sm')

        <source>Enter address or label to search</source>
        <translation>Tu'uina le atunu'u po'o le ata e su'e ai</translation>

<reply>UNK_LANG, not familiar with the Samoan language 'sm'.</reply>


# Example (erroneous 'de' translation)

        <source>Unable to open %s for writing</source>
        <translation>Konnte %s nicht zum Schreiben zu öffnen</translation>

<reply>ERR
The German grammar is incorrect. The verb 'öffnen' should be in the infinitive without 'zu' when used with the modal verb 'konnte'.

Correct translation:
Konnte %s nicht zum Schreiben öffnen
</reply>

# Example (spam 'de' translation)

        <source>Create a new address</source>
        <translation>&lt;br&gt;(</translation>

<reply>SPAM
Rather than providing a correct German translation, the response includes unrelated code.

Correct translation:
Neue Adresse erstellen
</reply>

# Example (spam 'de' translation)

        <source>Delete the currently selected address from the list</source>
        <translation>Usuń aktualnie wybrany adres z listy</translation>

<reply>SPAM
The translation is in Polish, not German as requested.

Correct translation:
Ausgewählte Adresse aus der Liste entfernen
</reply>


# Translation

Evaluate this '{lang}' translation:

{msg}

"#,
        );

        let cache_file = cache_dir.join(cache_key(lang, &msg));

        match fs::read_to_string(&cache_file) {
            Ok(contents) => {
                print_result(
                    &mut num_issues,
                    &cache_file,
                    &contents,
                    &prompt,
                    &msg,
                    report_file,
                );
            }
            Err(_) => {
                println!(
                    "Cache miss [file= {file}] for prompt=\n{prompt}",
                    file = cache_file.display()
                );
                let sleep_target = Instant::now() + rate_limit_wait;
                let payload = json!({
                  "model": model,
                  "messages": [
                    {"role": "user", "content": prompt}
                ]
                });

                let curl_out = Command::new("curl")
                    .arg("-X")
                    .arg("POST")
                    .arg("-H")
                    .arg("Content-Type: application/json")
                    .arg(url)
                    .arg("-H")
                    .arg(format!("Authorization: Bearer {token}"))
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
                let val = response["choices"][0]["message"]["content"]
                    .as_str()
                    .expect("Content not found")
                    .trim();
                fs::write(&cache_file, val).expect("Must be able to write cache file");
                print_result(
                    &mut num_issues,
                    &cache_file,
                    val,
                    &prompt,
                    &msg,
                    report_file,
                );
                sleep(sleep_target - Instant::now());
            }
        }
    }
    report_file
        .write_all(format!("</details>\n\nNumber of issues: {num_issues}.\n").as_bytes())
        .unwrap();
}
