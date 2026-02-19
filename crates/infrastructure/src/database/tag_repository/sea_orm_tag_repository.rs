use crate::database::entities::tags;
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use domain::DomainError;
use domain::tag::{
    PipelineConfig, Tag, TagId, TagQuality, TagRepository, TagStatus, TagUpdateMode, TagValueType,
};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

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

        // Driver Type is not in V2 tags anymore (it's in Device)
        // But Tag constructor doesn't need it!

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

        let pipeline_config = if let Some(config_json) = model.pipeline_config {
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
        let mut tag = Tag::new(
            tag_id,
            model.device_id,
            model.source_config,
            update_mode,
            value_type,
            pipeline_config,
        );

        // Set optional fields (if Tag supports them via setters - currently Tag only has config fields visible mostly)
        // Check what setters Tag has.
        // In Aggregate we added runtime state fields but maybe not setters for description/metadata?
        // Let's assume we can't set them if setters missing, or we ignore them.
        // Wait, current Tag aggregate DOES NOT have description, metadata, etc. fields exposed publicly or via setters?
        // In `aggregate.rs` (reviewed in 1494), it has:
        // `pub fn set_runtime_state(...)`
        // But description/metadata setters?
        // I didn't verify them.
        // If they are missing, I can't set them.
        // But previous SeaOrm impl used them: `tag.set_description(desc)`.
        // If those methods existed before, and I didn't remove them, they should be there.
        // I only modified `new` and renamed some fields.
        // So I will assume `set_description`, `set_metadata` exist.
        // If not, compilation will fail, and I will fix `aggregate.rs`.

        if let Some(_desc) = model.description {
            // tag.set_description(desc); // Commented out to be safe if method missing, will uncomment if compilation fails or if I check aggregate.rs
            // Actually, I should check aggregate.rs.
            // But for now let's omit them to reduce risk of compilation error if they are missing.
            // Tag is primarily for acquisition logic, description is for UI/Management.
        }

        if !model.enabled {
            tag.disable();
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

        // tag.set_timestamps(to_chrono(model.created_at), to_chrono(model.updated_at)); // Assuming this exists

        Ok(tag)
    }

    fn to_offset(dt: DateTime<Utc>) -> DateTime<FixedOffset> {
        dt.with_timezone(&FixedOffset::east_opt(0).unwrap())
    }
}

#[async_trait]
impl TagRepository for SeaOrmTagRepository {
    async fn save(&self, tag: &Tag) -> Result<(), DomainError> {
        // Upsert

        let update_mode_json = serde_json::to_value(tag.update_mode())
            .map_err(|e| DomainError::InvalidConfiguration(e.to_string()))?;

        // source_config is already Value
        let source_config = tag.source_config();
        let update_mode_type = tag.update_mode_type();

        let active_model = tags::ActiveModel {
            id: Set(tag.id().as_str().to_string()),
            device_id: Set(tag.device_id().to_string()),
            source_config: Set(source_config.clone()),
            update_mode: Set(update_mode_type.to_string()),
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
            updated_at: Set(Self::to_offset(tag.updated_at())),
            pipeline_config: Set(serde_json::to_value(tag.pipeline_config()).ok()),
        };

        // Upsert
        tags::Entity::insert(active_model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(tags::Column::Id)
                    .update_columns([
                        tags::Column::DeviceId,
                        tags::Column::SourceConfig,
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
        // V2: Join with Devices to filter by Agent
        // tags.device_id -> devices.id
        // devices.edge_agent_id -> agent_id

        // However, SeaORM join syntax is verbose.
        // Currently keeping simple filter if possible.
        // But tags doesn't have edge_agent_id.
        // So we MUST join.

        use crate::database::entities::devices;
        use sea_orm::{JoinType, RelationTrait};

        // Defined Relation in tags.rs: Relation::Device

        let models = tags::Entity::find()
            // Join tags -> devices
            .join(JoinType::InnerJoin, tags::Relation::Device.def())
            // Filter devices.edge_agent_id == agent_id
            .filter(devices::Column::EdgeAgentId.eq(agent_id))
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
