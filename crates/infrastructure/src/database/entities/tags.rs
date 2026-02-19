use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tags")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub driver_type: String,
    pub driver_config: Json,
    pub edge_agent_id: String,
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
        belongs_to = "super::edge_agents::Entity",
        from = "Column::EdgeAgentId",
        to = "super::edge_agents::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    EdgeAgent,
    #[sea_orm(has_many = "super::tag_history::Entity")]
    TagHistory,
}

impl Related<super::edge_agents::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::EdgeAgent.def()
    }
}

impl Related<super::tag_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TagHistory.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
