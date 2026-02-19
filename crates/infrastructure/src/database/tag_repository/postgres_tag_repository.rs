use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::driver::DriverType;
use domain::tag::{
    PipelineConfig, TagQuality, TagRepository, TagStatus, TagUpdateMode, TagValueType,
};
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
        let update_mode_json = serde_json::to_value(&tag.update_mode())
            .map_err(|e| DomainError::InvalidConfiguration(e.to_string()))?;

        let driver_config = tag.driver_config();

        sqlx::query!(
            r#"
            INSERT INTO tags (
                id, driver_type, driver_config, edge_agent_id,
                update_mode, update_config, value_type, value_schema,
                enabled, description, metadata,
                last_value, last_update, status, quality, error_message,
                created_at, updated_at, pipeline_config
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19
            )
            ON CONFLICT (id) DO UPDATE SET
                driver_type = EXCLUDED.driver_type,
                driver_config = EXCLUDED.driver_config,
                edge_agent_id = EXCLUDED.edge_agent_id,
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
            tag.driver_type().as_str(),
            driver_config,
            tag.edge_agent_id(),
            tag.update_mode_type(),
            update_mode_json,
            tag.value_type_str(),
            tag.value_schema(),
            tag.is_enabled(),
            tag.description(),
            tag.metadata(),
            // ... inside save method ...
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

    // ...

    async fn find_by_id(&self, id: &TagId) -> Result<Option<Tag>, DomainError> {
        let row = sqlx::query!(
            r#"
            SELECT id, driver_type, driver_config, edge_agent_id,
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
                    r.driver_type,
                    r.driver_config,
                    r.edge_agent_id,
                    r.update_mode,
                    r.update_config,
                    r.value_type,
                    r.value_schema,
                    r.enabled,
                    r.description,
                    r.metadata,
                    r.last_value,
                    r.last_update,
                    r.status,
                    r.quality,
                    r.error_message,
                    r.created_at,
                    r.updated_at,
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
            SELECT id, driver_type, driver_config, edge_agent_id,
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
                    r.driver_type,
                    r.driver_config,
                    r.edge_agent_id,
                    r.update_mode,
                    r.update_config,
                    r.value_type,
                    r.value_schema,
                    r.enabled,
                    r.description,
                    r.metadata,
                    r.last_value,
                    r.last_update,
                    r.status,
                    r.quality,
                    r.error_message,
                    r.created_at,
                    r.updated_at,
                    r.pipeline_config,
                )
            })
            .collect()
    }

    async fn find_by_agent(&self, agent_id: &str) -> Result<Vec<Tag>, DomainError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, driver_type, driver_config, edge_agent_id,
                   update_mode, update_config, value_type, value_schema,
                   enabled, description, metadata,
                   last_value, last_update, status, quality, error_message,
                   created_at, updated_at, pipeline_config
            FROM tags
            WHERE edge_agent_id = $1
            ORDER BY id
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
                    r.driver_type,
                    r.driver_config,
                    r.edge_agent_id,
                    r.update_mode,
                    r.update_config,
                    r.value_type,
                    r.value_schema,
                    r.enabled,
                    r.description,
                    r.metadata,
                    r.last_value,
                    r.last_update,
                    r.status,
                    r.quality,
                    r.error_message,
                    r.created_at,
                    r.updated_at,
                    r.pipeline_config,
                )
            })
            .collect()
    }

    async fn find_enabled(&self) -> Result<Vec<Tag>, DomainError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, driver_type, driver_config, edge_agent_id,
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
                    r.driver_type,
                    r.driver_config,
                    r.edge_agent_id,
                    r.update_mode,
                    r.update_config,
                    r.value_type,
                    r.value_schema,
                    r.enabled,
                    r.description,
                    r.metadata,
                    r.last_value,
                    r.last_update,
                    r.status,
                    r.quality,
                    r.error_message,
                    r.created_at,
                    r.updated_at,
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
        driver_type: String,
        driver_config: JsonValue,
        edge_agent_id: String,
        _update_mode: String,
        update_config: JsonValue,
        value_type: String,
        value_schema: Option<JsonValue>,
        enabled: bool,
        description: Option<String>,
        metadata: Option<JsonValue>,
        last_value: Option<JsonValue>,
        last_update: Option<OffsetDateTime>,
        status: String,
        quality: String,
        error_message: Option<String>,
        created_at: OffsetDateTime,
        updated_at: OffsetDateTime,
        pipeline_config: Option<JsonValue>,
    ) -> Result<Tag, DomainError> {
        // Helper to convert time::OffsetDateTime to chrono::DateTime<Utc>
        let to_chrono = |dt: OffsetDateTime| -> DateTime<Utc> {
            let timestamp = dt.unix_timestamp();
            let nanos = dt.nanosecond();
            DateTime::from_timestamp(timestamp, nanos).unwrap_or_default()
        };

        // Parse enums and value objects
        let tag_id = TagId::new(id)?;

        let driver_type = match driver_type.as_str() {
            "RS232" => DriverType::RS232,
            "Modbus" => DriverType::Modbus,
            "OPC-UA" => DriverType::OPCUA,
            "HTTP" => DriverType::HTTP,
            _ => {
                return Err(DomainError::InvalidConfiguration(
                    "Unknown driver type".to_string(),
                ));
            }
        };

        let update_mode: TagUpdateMode = serde_json::from_value(update_config)
            .map_err(|e| DomainError::InvalidConfiguration(e.to_string()))?;

        let value_type = match value_type.as_str() {
            "Simple" => TagValueType::Simple,
            "Composite" => TagValueType::Composite,
            _ => {
                return Err(DomainError::InvalidConfiguration(
                    "Unknown value type".to_string(),
                ));
            }
        };

        let tag_status = match status.as_str() {
            "online" => TagStatus::Online,
            "offline" => TagStatus::Offline,
            "error" => TagStatus::Error,
            "unknown" => TagStatus::Unknown,
            _ => TagStatus::Unknown,
        };

        let tag_quality = match quality.as_str() {
            "good" => TagQuality::Good,
            "bad" => TagQuality::Bad,
            "uncertain" => TagQuality::Uncertain,
            "timeout" => TagQuality::Timeout,
            _ => TagQuality::Uncertain,
        };

        // Create tag with builder pattern
        let mut tag = Tag::new(
            tag_id,
            driver_type,
            driver_config,
            edge_agent_id,
            update_mode,
            value_type,
        );

        // Set optional fields and runtime state
        if let Some(schema) = value_schema {
            tag.set_value_schema(schema);
        }

        if let Some(desc) = description {
            tag.set_description(desc);
        }

        if let Some(meta) = metadata {
            tag.set_metadata(meta);
        }

        if !enabled {
            tag.disable();
        }

        if let Some(config_json) = pipeline_config {
            if !config_json.is_null() {
                let config: PipelineConfig = serde_json::from_value(config_json).map_err(|e| {
                    DomainError::InvalidConfiguration(format!("Invalid pipeline config: {}", e))
                })?;
                tag.set_pipeline_config(config);
            }
        }

        // Set runtime state using internal setters (need to add these to Tag)
        tag.set_runtime_state(
            last_value,
            last_update.map(to_chrono),
            tag_status,
            tag_quality,
            error_message,
        );
        tag.set_timestamps(to_chrono(created_at), to_chrono(updated_at));

        Ok(tag)
    }
}
