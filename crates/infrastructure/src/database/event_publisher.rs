use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use domain::DomainEvent;
use domain::event::EventPublisher;
use sqlx::PgPool;

/// Event publisher implementation for PostgreSQL
pub struct PostgresEventPublisher {
    pool: Arc<PgPool>,
}

impl PostgresEventPublisher {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
    // ...

    fn to_offset(dt: chrono::DateTime<Utc>) -> time::OffsetDateTime {
        let timestamp = dt.timestamp();
        let nanos = dt.timestamp_subsec_nanos();
        time::OffsetDateTime::from_unix_timestamp_nanos(
            (timestamp as i128) * 1_000_000_000 + (nanos as i128),
        )
        .unwrap()
    }
}

#[async_trait]
impl EventPublisher for PostgresEventPublisher {
    async fn publish(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let DomainEvent::AgentHeartbeat { .. } = event {
            return Ok(());
        }

        let event_type = event.event_type();
        let payload = serde_json::to_value(&event)?;
        let occurred_at = Self::to_offset(Utc::now());

        sqlx::query!(
            r#"
            INSERT INTO events (event_type, payload, occurred_at)
            VALUES ($1, $2, $3)
            "#,
            event_type,
            payload,
            occurred_at
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    async fn publish_batch(
        &self,
        events: Vec<DomainEvent>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if events.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for event in events {
            if let DomainEvent::AgentHeartbeat { .. } = event {
                continue;
            }
            let event_type = event.event_type();
            let payload = serde_json::to_value(&event)?;
            let occurred_at = Self::to_offset(Utc::now());

            sqlx::query!(
                r#"
                INSERT INTO events (event_type, payload, occurred_at)
                VALUES ($1, $2, $3)
                "#,
                event_type,
                payload,
                occurred_at
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }
}
