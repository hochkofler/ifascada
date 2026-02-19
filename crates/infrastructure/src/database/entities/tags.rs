use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tags")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub device_id: String,
    pub source_config: Json,
    pub update_mode: String,
    pub update_config: Json,
    pub value_type: String,
    pub value_schema: Option<Json>,
    pub enabled: bool,
    pub description: Option<String>,
    pub metadata: Option<Json>,
    pub last_value: Option<Json>,
    pub last_update: Option<DateTimeWithTimeZone>,
    pub status: String,
    pub quality: String,
    pub error_message: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub pipeline_config: Option<Json>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::devices::Entity",
        from = "Column::DeviceId",
        to = "super::devices::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Device,
    #[sea_orm(has_many = "super::tag_history::Entity")]
    TagHistory,
}

impl Related<super::devices::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Device.def()
    }
}

impl Related<super::tag_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TagHistory.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
