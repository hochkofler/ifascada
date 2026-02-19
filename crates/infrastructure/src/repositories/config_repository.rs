use crate::config::TagConfig;
use anyhow::Result;
use async_trait::async_trait;
use domain::DomainError;
use domain::tag::{Tag, TagId, TagRepository, TagUpdateMode, TagValueType};

#[derive(Clone)]
pub struct ConfigTagRepository {
    agent_id: String,
    tags: Vec<Tag>,
}

impl ConfigTagRepository {
    pub fn new(agent_id: &str, tag_configs: Vec<TagConfig>) -> Self {
        let tags = tag_configs
            .into_iter()
            .filter_map(|cfg| {
                let mut pipeline = cfg.pipeline.unwrap_or_default();
                if !cfg.automations.is_empty() {
                    pipeline.automations = cfg.automations;
                }

                let tag_id = match TagId::new(&cfg.id) {
                    Ok(id) => id,
                    Err(e) => {
                        tracing::error!("Invalid Tag ID '{}': {}", cfg.id, e);
                        return None;
                    }
                };

                let device_id = match cfg.device_id {
                    Some(id) => id,
                    None => {
                        tracing::error!("Tag {} is missing required device_id. Skipping.", cfg.id);
                        return None;
                    }
                };

                let source_config = match cfg.driver_config {
                    Some(cfg) => cfg,
                    None => {
                        tracing::error!("Tag {} is missing source_config. Skipping.", cfg.id);
                        return None;
                    }
                };

                let mut tag = Tag::new(
                    tag_id,
                    device_id,
                    source_config,
                    cfg.update_mode
                        .unwrap_or(TagUpdateMode::Polling { interval_ms: 1000 }),
                    cfg.value_type.unwrap_or(TagValueType::Simple),
                    pipeline,
                );

                if let Some(enabled) = cfg.enabled {
                    if !enabled {
                        tag.disable();
                    }
                }

                Some(tag)
            })
            .collect();

        Self {
            agent_id: agent_id.to_string(),
            tags,
        }
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
        if self.agent_id == agent_id {
            Ok(self.tags.clone())
        } else {
            Ok(vec![])
        }
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
