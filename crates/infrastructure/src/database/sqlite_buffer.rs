use anyhow::Result;
use sqlx::{Pool, Row, Sqlite, sqlite::SqlitePoolOptions};

#[derive(Clone)]
pub struct SQLiteBuffer {
    pool: Pool<Sqlite>,
}

impl SQLiteBuffer {
    pub async fn new(connection_string: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1) // SQLite is single-writer
            .connect(connection_string)
            .await?;

        // Initialize table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS offline_buffer (
                id INTEGER PRIMARY KEY,
                topic TEXT NOT NULL,
                payload BLOB NOT NULL,
                created_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    pub async fn enqueue(&self, topic: &str, payload: &[u8]) -> Result<()> {
        sqlx::query("INSERT INTO offline_buffer (topic, payload, created_at) VALUES (?, ?, strftime('%s','now'))")
            .bind(topic)
            .bind(payload)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn dequeue_batch(&self, limit: i64) -> Result<Vec<(i64, String, Vec<u8>)>> {
        let rows = sqlx::query(
            "SELECT id, topic, payload FROM offline_buffer ORDER BY created_at ASC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut batch = Vec::new();
        for row in rows {
            batch.push((row.get(0), row.get(1), row.get(2)));
        }
        Ok(batch)
    }

    pub async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM offline_buffer WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn count(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM offline_buffer")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}
