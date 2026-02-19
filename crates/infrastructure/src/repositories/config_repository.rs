use crate::config::TagConfig;
use anyhow::Result;
use async_trait::async_trait;
use domain::DomainError;
use domain::tag::{Tag, TagId, TagRepository, TagUpdateMode, TagValueType};

#[derive(Clone)]
pub struct ConfigTagRepository {
    tags: Vec<Tag>,
}

impl ConfigTagRepository {
    pub fn new(agent_id: &str, tag_configs: Vec<TagConfig>) -> Self {
        let tags = tag_configs
            .into_iter()
            .map(|cfg| {
                let mut tag = Tag::new(
                    TagId::new(&cfg.id).unwrap(),
                    cfg.driver,
                    cfg.driver_config,
                    agent_id.to_string(),
                    cfg.update_mode
                        .unwrap_or(TagUpdateMode::Polling { interval_ms: 1000 }),
                    cfg.value_type.unwrap_or(TagValueType::Simple),
                );

                if let Some(enabled) = cfg.enabled {
                    if !enabled {
                        tag.disable();
                    }
                }

                let mut pipeline = cfg.pipeline.unwrap_or_default();
                if !cfg.automations.is_empty() {
                    pipeline.automations = cfg.automations;
                }
                tag.set_pipeline_config(pipeline);

                tag
            })
            .collect();

        Self { tags }
    }
}

#[async_trait]
impl TagRepository for ConfigTagRepository {
    async fn save(&self, _tag: &Tag) -> Result<(), DomainError> {
        // In-memory config is read-only for now
        Ok(())
    }

    async fn find_by_id(&self, id: &TagId) -> Result<Option<Tag>, DomainError> {
        Ok(self.tags.iter().find(|t| t.id() == id).cloned())
    }

    async fn find_all(&self) -> Result<Vec<Tag>, DomainError> {
        Ok(self.tags.clone())
    }

    async fn find_by_agent(&self, agent_id: &str) -> Result<Vec<Tag>, DomainError> {
        Ok(self
            .tags
            .iter()
            .filter(|t| t.edge_agent_id() == agent_id)
            .cloned()
            .collect())
    }

    async fn find_enabled(&self) -> Result<Vec<Tag>, DomainError> {
        Ok(self
            .tags
            .iter()
            .filter(|t| t.is_enabled())
            .cloned()
            .collect())
    }

    async fn delete(&self, _id: &TagId) -> Result<(), DomainError> {
        // Read-only
        Ok(())
    }
}
