use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct TicketSequenceStore {
    conn: Arc<Mutex<Connection>>,
}

impl TicketSequenceStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS edge_ticket_sequence (
                edge_code TEXT PRIMARY KEY,
                next_sequence INTEGER NOT NULL CHECK(next_sequence >= 1)
            );",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn next_ticket_id(&self, edge_code: &str) -> Result<String, String> {
        let edge_code = edge_code.trim();
        if edge_code.is_empty() {
            return Err("ticket edge code must not be empty".to_string());
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "ticket sequence lock poisoned".to_string())?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let current = tx
            .query_row(
                "SELECT next_sequence FROM edge_ticket_sequence WHERE edge_code = ?1",
                params![edge_code],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let allocated = current.unwrap_or(1);
        tx.execute(
            "INSERT INTO edge_ticket_sequence(edge_code, next_sequence)
             VALUES (?1, ?2)
             ON CONFLICT(edge_code) DO UPDATE SET next_sequence = excluded.next_sequence",
            params![edge_code, allocated + 1],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        format_ticket_id(edge_code, allocated)
    }
}

pub fn format_ticket_id(edge_code: &str, sequence: i64) -> Result<String, String> {
    let prefix = edge_code.trim();
    if prefix.is_empty() || sequence < 1 {
        return Err("ticket edge code and sequence must be positive".to_string());
    }
    Ok(format!("{}-{:07}", prefix.to_ascii_uppercase(), sequence))
}

#[cfg(test)]
mod tests {
    use super::{format_ticket_id, TicketSequenceStore};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}.db", name, stamp))
    }

    #[test]
    fn formats_lcc_ticket_with_uppercase_prefix_and_minimum_width() {
        assert_eq!(format_ticket_id("lcc01", 1).unwrap(), "LCC01-0000001");
        assert_eq!(
            format_ticket_id("lcc01", 1_000_000).unwrap(),
            "LCC01-1000000"
        );
    }

    #[test]
    fn next_ticket_is_monotonic_after_reopening_the_database() {
        let path = temp_file("ticket_sequence");
        let first = TicketSequenceStore::open(&path).unwrap();
        assert_eq!(first.next_ticket_id("lcc01").unwrap(), "LCC01-0000001");
        drop(first);

        let reopened = TicketSequenceStore::open(&path).unwrap();
        assert_eq!(reopened.next_ticket_id("lcc01").unwrap(), "LCC01-0000002");
        drop(reopened);
        let _ = std::fs::remove_file(path);
    }
}
