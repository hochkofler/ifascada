use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create edge_agents table
        manager
            .create_table(
                Table::create()
                    .table(EdgeAgents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EdgeAgents::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(EdgeAgents::Description).string())
                    .col(
                        ColumnDef::new(EdgeAgents::Status)
                            .string()
                            .default("unknown"),
                    )
                    .col(ColumnDef::new(EdgeAgents::LastHeartbeat).timestamp_with_time_zone())
                    .col(ColumnDef::new(EdgeAgents::Metadata).json_binary())
                    .col(
                        ColumnDef::new(EdgeAgents::CreatedAt)
                            .timestamp_with_time_zone()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(EdgeAgents::UpdatedAt)
                            .timestamp_with_time_zone()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Create tags table
        manager
            .create_table(
                Table::create()
                    .table(Tags::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Tags::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Tags::DriverType).string().not_null())
                    .col(ColumnDef::new(Tags::DriverConfig).json_binary().not_null())
                    .col(ColumnDef::new(Tags::EdgeAgentId).string().not_null())
                    .col(ColumnDef::new(Tags::UpdateMode).string().not_null())
                    .col(ColumnDef::new(Tags::UpdateConfig).json_binary().not_null())
                    .col(ColumnDef::new(Tags::ValueType).string().not_null())
                    .col(ColumnDef::new(Tags::ValueSchema).json_binary())
                    .col(
                        ColumnDef::new(Tags::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(ColumnDef::new(Tags::Description).string())
                    .col(ColumnDef::new(Tags::Metadata).json_binary())
                    .col(ColumnDef::new(Tags::LastValue).json_binary())
                    .col(ColumnDef::new(Tags::LastUpdate).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Tags::Status)
                            .string()
                            .not_null()
                            .default("unknown"),
                    )
                    .col(
                        ColumnDef::new(Tags::Quality)
                            .string()
                            .not_null()
                            .default("uncertain"),
                    )
                    .col(ColumnDef::new(Tags::ErrorMessage).string())
                    .col(
                        ColumnDef::new(Tags::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Tags::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_edge_agent")
                            .from(Tags::Table, Tags::EdgeAgentId)
                            .to(EdgeAgents::Table, EdgeAgents::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create tag_history table
        manager
            .create_table(
                Table::create()
                    .table(TagHistory::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TagHistory::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(TagHistory::TagId).string().not_null())
                    .col(ColumnDef::new(TagHistory::Value).json_binary().not_null())
                    .col(ColumnDef::new(TagHistory::Quality).string().not_null())
                    .col(
                        ColumnDef::new(TagHistory::Timestamp)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_tag")
                            .from(TagHistory::Table, TagHistory::TagId)
                            .to(Tags::Table, Tags::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on tag_history (tag_id, timestamp DESC)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_tag_history_tag_time")
                    .table(TagHistory::Table)
                    .col(TagHistory::TagId)
                    .col(TagHistory::Timestamp)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(TagHistory::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Tags::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(EdgeAgents::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum EdgeAgents {
    Table,
    Id,
    Description,
    Status,
    LastHeartbeat,
    Metadata,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Tags {
    Table,
    Id,
    DriverType,
    DriverConfig,
    EdgeAgentId,
    UpdateMode,
    UpdateConfig,
    ValueType,
    ValueSchema,
    Enabled,
    Description,
    Metadata,
    LastValue,
    LastUpdate,
    Status,
    Quality,
    ErrorMessage,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum TagHistory {
    Table,
    Id,
    TagId,
    Value,
    Quality,
    Timestamp,
}
