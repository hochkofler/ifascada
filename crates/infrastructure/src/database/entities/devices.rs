use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "devices")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub edge_agent_id: String,
    pub name: String,
    pub driver_type: String,     // Moved from Tags
    pub connection_config: Json, // Moved from Tags (was driver_config)
    pub enabled: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
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
    #[sea_orm(has_many = "super::tags::Entity")]
    Tags,
}

impl Related<super::edge_agents::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::EdgeAgent.def()
    }
}

impl Related<super::tags::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tags.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
