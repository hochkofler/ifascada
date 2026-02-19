use crate::database::entities::tags;
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use domain::tag::{
    PipelineConfig, Tag, TagId, TagQuality, TagRepository, TagStatus, TagUpdateMode, TagValueType,
};
use domain::{DomainError, driver::DriverType};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set};

pub struct SeaOrmTagRepository {
    db: DatabaseConnection,
}

impl SeaOrmTagRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    fn model_to_tag(&self, model: tags::Model) -> Result<Tag, DomainError> {
        // Parse enums and value objects
        let tag_id = TagId::new(model.id)?;

        let driver_type = match model.driver_type.as_str() {
            "RS232" => DriverType::RS232,
            "Simulator" => DriverType::Simulator,
            "Modbus" => DriverType::Modbus,
            "OPC-UA" => DriverType::OPCUA,
            "HTTP" => DriverType::HTTP,
            _ => {
                return Err(DomainError::InvalidConfiguration(
                    "Unknown driver type".to_string(),
                ));
            }
        };

        let update_mode: TagUpdateMode = serde_json::from_value(model.update_config)
            .map_err(|e| DomainError::InvalidConfiguration(e.to_string()))?;

        let value_type = match model.value_type.as_str() {
            "Simple" => TagValueType::Simple,
            "Composite" => TagValueType::Composite,
            _ => {
                return Err(DomainError::InvalidConfiguration(
                    "Unknown value type".to_string(),
                ));
            }
        };

        // Create tag
        let mut tag = Tag::new(
            tag_id,
            driver_type,
            model.driver_config,
            model.edge_agent_id,
            update_mode,
            value_type,
        );

        // Set optional fields
        if let Some(schema) = model.value_schema {
            tag.set_value_schema(schema);
        }
        if let Some(desc) = model.description {
            tag.set_description(desc);
        }
        if let Some(meta) = model.metadata {
            tag.set_metadata(meta);
        }
        if !model.enabled {
            tag.disable();
        }

        // Pipeline config
        if let Some(config_json) = model.pipeline_config {
            if !config_json.is_null() {
                let config: PipelineConfig = serde_json::from_value(config_json).map_err(|e| {
                    DomainError::InvalidConfiguration(format!("Invalid pipeline config: {}", e))
                })?;
                tag.set_pipeline_config(config);
            }
        }

        // Runtime state
        let status = match model.status.as_str() {
            "online" => TagStatus::Online,
            "offline" => TagStatus::Offline,
            "error" => TagStatus::Error,
            _ => TagStatus::Unknown,
        };

        let quality = match model.quality.as_str() {
            "good" => TagQuality::Good,
            "bad" => TagQuality::Bad,
            "uncertain" => TagQuality::Uncertain,
            "timeout" => TagQuality::Timeout,
            _ => TagQuality::Uncertain,
        };

        // Convert timestamps from DateTime<FixedOffset> to DateTime<Utc>
        let to_chrono = |dt: DateTime<FixedOffset>| -> DateTime<Utc> { dt.with_timezone(&Utc) };

        let to_chrono_opt =
            |dt: Option<DateTime<FixedOffset>>| -> Option<DateTime<Utc>> { dt.map(to_chrono) };

        tag.set_runtime_state(
            model.last_value,
            to_chrono_opt(model.last_update),
            status,
            quality,
            model.error_message,
        );

        tag.set_timestamps(to_chrono(model.created_at), to_chrono(model.updated_at));

        Ok(tag)
    }

    fn to_offset(dt: DateTime<Utc>) -> DateTime<FixedOffset> {
        dt.with_timezone(&FixedOffset::east_opt(0).unwrap())
    }
}

#[async_trait]
impl TagRepository for SeaOrmTagRepository {
    async fn save(&self, tag: &Tag) -> Result<(), DomainError> {
        // Convert Tag to ActiveModel
        // Check if exists? Or use ON CONFLICT logic if supported by DB/ORM?
        // SeaORM supports `insert` with `on_conflict`.

        let update_mode_json = serde_json::to_value(&tag.update_mode())
            .map_err(|e| DomainError::InvalidConfiguration(e.to_string()))?;

        let active_model = tags::ActiveModel {
            id: Set(tag.id().as_str().to_string()),
            driver_type: Set(tag.driver_type().as_str().to_string()),
            driver_config: Set(tag.driver_config().clone()),
            edge_agent_id: Set(tag.edge_agent_id().to_string()),
            update_mode: Set(tag.update_mode_type().to_string()),
            update_config: Set(update_mode_json),
            value_type: Set(tag.value_type_str().to_string()),
            value_schema: Set(tag.value_schema().map(|v| v.clone())),
            enabled: Set(tag.is_enabled()),
            description: Set(tag.description().map(|s| s.to_string())),
            metadata: Set(tag.metadata().map(|v| v.clone())),
            last_value: Set(tag.last_value().map(|v| v.clone())),
            last_update: Set(tag.last_update().map(Self::to_offset)),
            status: Set(tag.status().as_str().to_string()),
            quality: Set(tag.quality().as_str().to_string()),
            error_message: Set(tag.error_message().map(|s| s.to_string())),
            created_at: Set(Self::to_offset(tag.created_at())),
            updated_at: Set(Self::to_offset(tag.updated_at())), // typically updated_at is auto-set by DB trigger but we allow manual override here or sync
            pipeline_config: Set(serde_json::to_value(tag.pipeline_config()).ok()),
        };

        // Upsert
        tags::Entity::insert(active_model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(tags::Column::Id)
                    .update_columns([
                        tags::Column::DriverType,
                        tags::Column::DriverConfig,
                        tags::Column::EdgeAgentId,
                        tags::Column::UpdateMode,
                        tags::Column::UpdateConfig,
                        tags::Column::ValueType,
                        tags::Column::ValueSchema,
                        tags::Column::Enabled,
                        tags::Column::Description,
                        tags::Column::Metadata,
                        tags::Column::LastValue,
                        tags::Column::LastUpdate,
                        tags::Column::Status,
                        tags::Column::Quality,
                        tags::Column::ErrorMessage,
                        tags::Column::UpdatedAt,
                        tags::Column::PipelineConfig,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &TagId) -> Result<Option<Tag>, DomainError> {
        let model = tags::Entity::find_by_id(id.as_str().to_string())
            .one(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        match model {
            Some(m) => Ok(Some(self.model_to_tag(m)?)),
            None => Ok(None),
        }
    }

    async fn find_all(&self) -> Result<Vec<Tag>, DomainError> {
        let models = tags::Entity::find()
            .order_by_asc(tags::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        let mut result = Vec::new();
        for m in models {
            result.push(self.model_to_tag(m)?);
        }
        Ok(result)
    }

    async fn find_by_agent(&self, agent_id: &str) -> Result<Vec<Tag>, DomainError> {
        let models = tags::Entity::find()
            .filter(tags::Column::EdgeAgentId.eq(agent_id))
            .order_by_asc(tags::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        let mut result = Vec::new();
        for m in models {
            result.push(self.model_to_tag(m)?);
        }
        Ok(result)
    }

    async fn find_enabled(&self) -> Result<Vec<Tag>, DomainError> {
        let models = tags::Entity::find()
            .filter(tags::Column::Enabled.eq(true))
            .order_by_asc(tags::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        let mut result = Vec::new();
        for m in models {
            result.push(self.model_to_tag(m)?);
        }
        Ok(result)
    }

    async fn delete(&self, id: &TagId) -> Result<(), DomainError> {
        tags::Entity::delete_by_id(id.as_str())
            .exec(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        Ok(())
    }
}
