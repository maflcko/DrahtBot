#[derive(serde::Deserialize)]
pub struct Repo {
    pub repo_slug: String,
    pub backport_label: String,
    pub repo_labels: std::collections::HashMap<String, Vec<String>>,
    pub corecheck: bool,
}

#[derive(serde::Deserialize)]
pub struct Config {
    pub repositories: Vec<Repo>,
}
