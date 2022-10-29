pub async fn get_pull_mergeable(
    api: &octocrab::pulls::PullRequestHandler<'_>,
    number: u64,
) -> octocrab::Result<Option<octocrab::models::pulls::PullRequest>> {
    // https://docs.github.com/en/rest/guides/getting-started-with-the-git-database-api#checking-mergeability-of-pull-requests
    loop {
        let pull = api.get(number).await?;
        if pull.state.as_ref().unwrap() != &octocrab::models::IssueState::Open {
            return Ok(None);
        }
        if pull.mergeable.is_none() {
            std::thread::sleep(std::time::Duration::from_secs(3));
            continue;
        }
        return Ok(Some(pull));
    }
}
