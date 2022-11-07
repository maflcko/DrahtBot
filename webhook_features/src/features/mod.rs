pub mod summary_comment;

use crate::errors::Result;
use crate::Context;
use crate::GitHubEvent;
use async_trait::async_trait;

pub struct FeatureMeta {
    name: &'static str,
    description: &'static str,
    events: Vec<GitHubEvent>,
}

impl FeatureMeta {
    pub fn new(name: &'static str, description: &'static str, events: Vec<GitHubEvent>) -> Self {
        Self {
            name,
            description,
            events,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn description(&self) -> &'static str {
        self.description
    }

    pub fn events(&self) -> &Vec<GitHubEvent> {
        &self.events
    }
}

#[async_trait]
pub trait Feature {
    fn meta(&self) -> &FeatureMeta;
    async fn handle(
        &self,
        ctx: &Context,
        event: GitHubEvent,
        payload: &serde_json::Value,
    ) -> Result<()>;
}
