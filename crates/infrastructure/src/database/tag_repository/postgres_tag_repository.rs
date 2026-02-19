use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::tag::{PipelineConfig, TagRepository, TagUpdateMode, TagValueType};
use domain::{DomainError, Tag, TagId};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use time::OffsetDateTime;

#[allow(dead_code)]
/// PostgreSQL implementation of TagRepository
pub struct PostgresTagRepository {
    pool: PgPool,
}

impl PostgresTagRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn to_offset(dt: DateTime<Utc>) -> OffsetDateTime {
        let timestamp = dt.timestamp();
        let nanos = dt.timestamp_subsec_nanos();
        OffsetDateTime::from_unix_timestamp_nanos(
            (timestamp as i128) * 1_000_000_000 + (nanos as i128),
        )
        .unwrap()
    }
}

#[async_trait]
impl TagRepository for PostgresTagRepository {
    async fn save(&self, tag: &Tag) -> Result<(), DomainError> {
        // Serialize complex types to JSON
        let update_mode_json = serde_json::to_value(tag.update_mode())
            .map_err(|e| DomainError::InvalidConfiguration(e.to_string()))?;

        // source_config is already Value
        let source_config = tag.source_config();

        let update_mode_type = tag.update_mode_type();

        sqlx::query!(
            r#"
            INSERT INTO tags (
                id, device_id, source_config,
                update_mode, update_config, value_type, value_schema,
                enabled, description, metadata,
                last_value, last_update, status, quality, error_message,
                created_at, updated_at, pipeline_config
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18
            )
            ON CONFLICT (id) DO UPDATE SET
                device_id = EXCLUDED.device_id,
                source_config = EXCLUDED.source_config,
                update_mode = EXCLUDED.update_mode,
                update_config = EXCLUDED.update_config,
                value_type = EXCLUDED.value_type,
                value_schema = EXCLUDED.value_schema,
                enabled = EXCLUDED.enabled,
                last_value = EXCLUDED.last_value,
                last_update = EXCLUDED.last_update,
                status = EXCLUDED.status,
                quality = EXCLUDED.quality,
                error_message = EXCLUDED.error_message,
                updated_at = EXCLUDED.updated_at,
                pipeline_config = EXCLUDED.pipeline_config
            "#,
            tag.id().as_str(),
            tag.device_id(),
            *source_config,
            update_mode_type,
            update_mode_json,
            tag.value_type_str(), // Use string representation
            tag.value_schema(),
            tag.is_enabled(),
            tag.description(),
            tag.metadata(),
            tag.last_value(),
            tag.last_update().map(Self::to_offset),
            tag.status().as_str(),
            tag.quality().as_str(),
            tag.error_message(),
            Self::to_offset(tag.created_at()),
            Self::to_offset(tag.updated_at()),
            serde_json::to_value(tag.pipeline_config()).ok()
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &TagId) -> Result<Option<Tag>, DomainError> {
        let row = sqlx::query!(
            r#"
            SELECT id, device_id, source_config,
                   update_mode, update_config, value_type, value_schema,
                   enabled, description, metadata,
                   last_value, last_update, status, quality, error_message,
                   created_at, updated_at, pipeline_config
            FROM tags
            WHERE id = $1
            "#,
            id.as_str()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        match row {
            Some(r) => {
                let tag = self.row_to_tag(
                    r.id,
                    r.device_id,
                    r.source_config,
                    r.update_config,
                    r.value_type,
                    r.pipeline_config,
                )?;
                Ok(Some(tag))
            }
            None => Ok(None),
        }
    }

    async fn find_all(&self) -> Result<Vec<Tag>, DomainError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, device_id, source_config,
                   update_mode, update_config, value_type, value_schema,
                   enabled, description, metadata,
                   last_value, last_update, status, quality, error_message,
                   created_at, updated_at, pipeline_config
            FROM tags
            ORDER BY id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        rows.into_iter()
            .map(|r| {
                self.row_to_tag(
                    r.id,
                    r.device_id,
                    r.source_config,
                    r.update_config,
                    r.value_type,
                    r.pipeline_config,
                )
            })
            .collect()
    }

    async fn find_by_agent(&self, agent_id: &str) -> Result<Vec<Tag>, DomainError> {
        // V2: Tags are linked to Devices, Devices linked to Agent.
        // So we need a JOIN.
        let rows = sqlx::query!(
            r#"
            SELECT t.id, t.device_id, t.source_config,
                   t.update_mode, t.update_config, t.value_type, t.value_schema,
                   t.enabled, t.description, t.metadata,
                   t.last_value, t.last_update, t.status, t.quality, t.error_message,
                   t.created_at, t.updated_at, t.pipeline_config
            FROM tags t
            JOIN devices d ON t.device_id = d.id
            WHERE d.edge_agent_id = $1
            ORDER BY t.id
            "#,
            agent_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        rows.into_iter()
            .map(|r| {
                self.row_to_tag(
                    r.id,
                    r.device_id,
                    r.source_config,
                    r.update_config,
                    r.value_type,
                    r.pipeline_config,
                )
            })
            .collect()
    }

    async fn find_enabled(&self) -> Result<Vec<Tag>, DomainError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, device_id, source_config,
                   update_mode, update_config, value_type, value_schema,
                   enabled, description, metadata,
                   last_value, last_update, status, quality, error_message,
                   created_at, updated_at, pipeline_config
            FROM tags
            WHERE enabled = true
            ORDER BY id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        rows.into_iter()
            .map(|r| {
                self.row_to_tag(
                    r.id,
                    r.device_id,
                    r.source_config,
                    r.update_config,
                    r.value_type,
                    r.pipeline_config,
                )
            })
            .collect()
    }

    async fn delete(&self, id: &TagId) -> Result<(), DomainError> {
        sqlx::query!(
            r#"
            DELETE FROM tags WHERE id = $1
            "#,
            id.as_str()
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        Ok(())
    }
}

// Helper methods for conversion
impl PostgresTagRepository {
    #[allow(clippy::too_many_arguments)]
    fn row_to_tag(
        &self,
        id: String,
        device_id: String,
        source_config: JsonValue,
        update_config: JsonValue,
        value_type: String,
        pipeline_config: Option<JsonValue>,
    ) -> Result<Tag, DomainError> {
        // Parse enums and value objects
        let tag_id = TagId::new(id)?;

        let update_mode: TagUpdateMode = serde_json::from_value(update_config)
            .map_err(|e| DomainError::InvalidConfiguration(e.to_string()))?;

        let value_type = match value_type.as_str() {
            "Simple" => TagValueType::Simple,
            "Composite" => TagValueType::Composite,
            _ => {
                return Err(DomainError::InvalidConfiguration(format!(
                    "Unknown value type: {}",
                    value_type
                )));
            }
        };

        let pipeline_config = if let Some(config_json) = pipeline_config {
            if !config_json.is_null() {
                serde_json::from_value(config_json).map_err(|e| {
                    DomainError::InvalidConfiguration(format!("Invalid pipeline config: {}", e))
                })?
            } else {
                PipelineConfig::default()
            }
        } else {
            PipelineConfig::default()
        };

        // Create tag
        Ok(Tag::new(
            tag_id,
            device_id,
            source_config,
            update_mode,
            value_type,
            pipeline_config,
        ))
    }
}
