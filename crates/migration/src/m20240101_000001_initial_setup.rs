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

        // Create devices table
        manager
            .create_table(
                Table::create()
                    .table(Devices::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Devices::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Devices::EdgeAgentId).string().not_null())
                    .col(ColumnDef::new(Devices::Name).string().not_null())
                    .col(ColumnDef::new(Devices::DriverType).string().not_null())
                    .col(
                        ColumnDef::new(Devices::ConnectionConfig)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Devices::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Devices::CreatedAt)
                            .timestamp_with_time_zone()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Devices::UpdatedAt)
                            .timestamp_with_time_zone()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_device_agent")
                            .from(Devices::Table, Devices::EdgeAgentId)
                            .to(EdgeAgents::Table, EdgeAgents::Id)
                            .on_delete(ForeignKeyAction::Cascade),
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
                    .col(ColumnDef::new(Tags::SourceConfig).json_binary().not_null())
                    .col(ColumnDef::new(Tags::DeviceId).string().not_null())
                    .col(ColumnDef::new(Tags::UpdateMode).string().not_null())
                    .col(ColumnDef::new(Tags::UpdateConfig).json_binary().not_null())
                    .col(ColumnDef::new(Tags::ValueType).string().not_null())
                    .col(ColumnDef::new(Tags::ValueSchema).json_binary())
                    .col(ColumnDef::new(Tags::PipelineConfig).json_binary())
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
                            .name("fk_tag_device")
                            .from(Tags::Table, Tags::DeviceId)
                            .to(Devices::Table, Devices::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create reports table
        manager
            .create_table(
                Table::create()
                    .table(Reports::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Reports::Id)
                            .uuid() // Using UUID for Report ID
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Reports::AgentId).string().not_null())
                    .col(
                        ColumnDef::new(Reports::StartTime)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Reports::EndTime)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Reports::TotalValue).json_binary())
                    .col(
                        ColumnDef::new(Reports::CreatedAt)
                            .timestamp_with_time_zone()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(Reports::ReportId).string()) // Legacy/External ID support
                    .to_owned(),
            )
            .await?;

        // Create report_items table
        manager
            .create_table(
                Table::create()
                    .table(ReportItems::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ReportItems::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ReportItems::ReportId).uuid().not_null())
                    .col(ColumnDef::new(ReportItems::TagId).string().not_null())
                    .col(ColumnDef::new(ReportItems::Value).json_binary().not_null())
                    .col(
                        ColumnDef::new(ReportItems::Timestamp)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_report_item_report")
                            .from(ReportItems::Table, ReportItems::ReportId)
                            .to(Reports::Table, Reports::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create tag_events table (Replacing TagHistory)
        manager
            .create_table(
                Table::create()
                    .table(TagEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TagEvents::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(TagEvents::TagId).string().not_null())
                    .col(ColumnDef::new(TagEvents::Value).json_binary().not_null())
                    .col(ColumnDef::new(TagEvents::Quality).string().not_null())
                    .col(
                        ColumnDef::new(TagEvents::Timestamp)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(TagEvents::CreatedAt)
                            .timestamp_with_time_zone()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_tag_event_tag")
                            .from(TagEvents::Table, TagEvents::TagId)
                            .to(Tags::Table, Tags::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on tag_events (tag_id, timestamp DESC)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_tag_events_tag_time")
                    .table(TagEvents::Table)
                    .col(TagEvents::TagId)
                    .col(TagEvents::Timestamp)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ReportItems::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Reports::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(TagEvents::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Tags::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Devices::Table).to_owned())
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
    SourceConfig, // Replaces DriverType/Config
    DeviceId,
    UpdateMode,
    UpdateConfig,
    ValueType,
    ValueSchema,
    PipelineConfig,
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
enum TagEvents {
    // Renamed from TagHistory
    Table,
    Id,
    TagId,
    Value,
    Quality,
    Timestamp,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Devices {
    Table,
    Id,
    EdgeAgentId,
    Name,
    DriverType,
    ConnectionConfig,
    Enabled,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Reports {
    Table,
    Id,
    ReportId,
    AgentId,
    StartTime,
    EndTime,
    TotalValue,
    CreatedAt,
}

#[derive(DeriveIden)]
enum ReportItems {
    Table,
    Id,
    ReportId,
    TagId,
    Value,
    Timestamp,
}
