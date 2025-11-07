#[derive(serde::Deserialize)]
pub struct Repo {
    pub repo_slug: String,
    pub backport_label: Option<String>,
    pub repo_labels: std::collections::HashMap<String, Vec<String>>,
    pub spam_detection: bool,
    pub ci_status: bool,
    pub corecheck: bool,
}

#[derive(serde::Deserialize)]
pub struct Config {
    pub repositories: Vec<Repo>,
}
