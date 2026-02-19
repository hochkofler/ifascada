use crate::database::entities::reports;
use async_trait::async_trait;
use domain::DomainEvent;
use domain::event::EventPublisher;
use sea_orm::{DatabaseConnection, EntityTrait, Set};

pub struct DatabaseEventPublisher {
    db: DatabaseConnection,
}

impl DatabaseEventPublisher {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl EventPublisher for DatabaseEventPublisher {
    async fn publish(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let DomainEvent::ReportCompleted {
            report_id,
            agent_id,
            items,
            timestamp,
        } = event
        {
            let items_json = serde_json::to_value(&items)?;

            let model = reports::ActiveModel {
                id: Set(report_id),
                agent_id: Set(agent_id),
                items: Set(items_json),
                timestamp: Set(timestamp.with_timezone(&chrono::FixedOffset::east_opt(0).unwrap())),
            };

            reports::Entity::insert(model)
                .on_conflict(
                    sea_orm::sea_query::OnConflict::column(reports::Column::Id)
                        .update_columns([reports::Column::Items, reports::Column::Timestamp])
                        .to_owned(),
                )
                .exec(&self.db)
                .await?;

            tracing::info!("💾 Report saved to local database");
        }
        Ok(())
    }
}
