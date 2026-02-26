use crate::runtime::event_bus::WriteCommandOutcome;
use chrono::{DateTime, Utc};
use domain::error::DomainError;
use domain::id::{ConnectionId, TagId};
use domain::tag::TagValue;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

pub trait WriteAuditRepository: Send + Sync {
    fn append(&self, record: WriteAuditRecord) -> Result<(), DomainError>;
    fn all(&self) -> Result<Vec<WriteAuditRecord>, DomainError>;
    fn by_tag(&self, tag_id: &TagId) -> Result<Vec<WriteAuditRecord>, DomainError>;
    fn by_command_id(&self, command_id: &str) -> Result<Vec<WriteAuditRecord>, DomainError>;
    fn query(&self, query: &WriteAuditQuery) -> Result<Vec<WriteAuditRecord>, DomainError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteAuditRecord {
    pub connection_id: Option<ConnectionId>,
    pub tag_id: TagId,
    pub command_id: Option<String>,
    pub value: TagValue,
    pub outcome: WriteCommandOutcome,
    pub reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct WriteAuditQuery {
    pub connection_id: Option<ConnectionId>,
    pub tag_id: Option<TagId>,
    pub command_id: Option<String>,
    pub outcome: Option<WriteCommandOutcome>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub offset: usize,
    pub limit: Option<usize>,
}

#[derive(Clone, Default)]
pub struct WriteAuditStore {
    records: Arc<RwLock<Vec<WriteAuditRecord>>>,
}

impl WriteAuditStore {
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn append_internal(&self, record: WriteAuditRecord) {
        self.records
            .write()
            .expect("write audit store lock poisoned")
            .push(record);
    }

    fn all_internal(&self) -> Vec<WriteAuditRecord> {
        self.records
            .read()
            .expect("write audit store lock poisoned")
            .clone()
    }

    fn by_tag_internal(&self, tag_id: &TagId) -> Vec<WriteAuditRecord> {
        self.records
            .read()
            .expect("write audit store lock poisoned")
            .iter()
            .filter(|r| &r.tag_id == tag_id)
            .cloned()
            .collect()
    }

    fn by_command_id_internal(&self, command_id: &str) -> Vec<WriteAuditRecord> {
        self.records
            .read()
            .expect("write audit store lock poisoned")
            .iter()
            .filter(|r| r.command_id.as_deref() == Some(command_id))
            .cloned()
            .collect()
    }

    fn query_internal(&self, query: &WriteAuditQuery) -> Vec<WriteAuditRecord> {
        apply_query(self.all_internal(), query)
    }
}

impl WriteAuditRepository for WriteAuditStore {
    fn append(&self, record: WriteAuditRecord) -> Result<(), DomainError> {
        self.append_internal(record);
        Ok(())
    }

    fn all(&self) -> Result<Vec<WriteAuditRecord>, DomainError> {
        Ok(self.all_internal())
    }

    fn by_tag(&self, tag_id: &TagId) -> Result<Vec<WriteAuditRecord>, DomainError> {
        Ok(self.by_tag_internal(tag_id))
    }

    fn by_command_id(&self, command_id: &str) -> Result<Vec<WriteAuditRecord>, DomainError> {
        Ok(self.by_command_id_internal(command_id))
    }

    fn query(&self, query: &WriteAuditQuery) -> Result<Vec<WriteAuditRecord>, DomainError> {
        Ok(self.query_internal(query))
    }
}

fn apply_query(records: Vec<WriteAuditRecord>, query: &WriteAuditQuery) -> Vec<WriteAuditRecord> {
    let mut filtered: Vec<WriteAuditRecord> = records
        .into_iter()
        .filter(|r| {
            if let Some(conn_id) = query.connection_id.as_ref() {
                if r.connection_id.as_ref() != Some(conn_id) {
                    return false;
                }
            }
            if let Some(tag_id) = query.tag_id.as_ref() {
                if &r.tag_id != tag_id {
                    return false;
                }
            }
            if let Some(command_id) = query.command_id.as_ref() {
                if r.command_id.as_ref() != Some(command_id) {
                    return false;
                }
            }
            if let Some(outcome) = query.outcome {
                if r.outcome != outcome {
                    return false;
                }
            }
            if let Some(from) = query.from {
                if r.timestamp < from {
                    return false;
                }
            }
            if let Some(to) = query.to {
                if r.timestamp > to {
                    return false;
                }
            }
            true
        })
        .collect();

    if query.offset > 0 {
        filtered = filtered.into_iter().skip(query.offset).collect();
    }
    if let Some(limit) = query.limit {
        filtered.truncate(limit);
    }
    filtered
}
