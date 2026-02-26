use application::runtime::{WriteAuditQuery, WriteAuditRecord, WriteAuditRepository};
use domain::error::DomainError;
use domain::id::TagId;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub struct JsonlWriteAuditRepository {
    path: PathBuf,
    records: RwLock<Vec<WriteAuditRecord>>,
}

impl JsonlWriteAuditRepository {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, DomainError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                DomainError::ConfigurationError(format!(
                    "failed to create write audit directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        if !path.exists() {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| {
                    DomainError::ConfigurationError(format!(
                        "failed to initialize write audit file {}: {}",
                        path.display(),
                        e
                    ))
                })?;
        }

        let file = OpenOptions::new()
            .read(true)
            .open(&path)
            .map_err(|e| {
                DomainError::ConfigurationError(format!(
                    "failed to open write audit file {}: {}",
                    path.display(),
                    e
                ))
            })?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|e| {
                DomainError::ConfigurationError(format!(
                    "failed to read write audit file {}: {}",
                    path.display(),
                    e
                ))
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let rec: WriteAuditRecord = serde_json::from_str(&line).map_err(|e| {
                DomainError::ConfigurationError(format!(
                    "failed to parse write audit json line in {}: {}",
                    path.display(),
                    e
                ))
            })?;
            records.push(rec);
        }

        Ok(Self {
            path,
            records: RwLock::new(records),
        })
    }
}

impl WriteAuditRepository for JsonlWriteAuditRepository {
    fn append(&self, record: WriteAuditRecord) -> Result<(), DomainError> {
        let payload = serde_json::to_string(&record).map_err(|e| {
            DomainError::ConfigurationError(format!("failed to serialize write audit record: {}", e))
        })?;

        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(|e| {
                DomainError::ConfigurationError(format!(
                    "failed to open write audit file {} for append: {}",
                    self.path.display(),
                    e
                ))
            })?;
        writeln!(file, "{}", payload).map_err(|e| {
            DomainError::ConfigurationError(format!(
                "failed to append write audit record to {}: {}",
                self.path.display(),
                e
            ))
        })?;

        self.records
            .write()
            .expect("write audit repository lock poisoned")
            .push(record);
        Ok(())
    }

    fn all(&self) -> Result<Vec<WriteAuditRecord>, DomainError> {
        Ok(self
            .records
            .read()
            .expect("write audit repository lock poisoned")
            .clone())
    }

    fn by_tag(&self, tag_id: &TagId) -> Result<Vec<WriteAuditRecord>, DomainError> {
        Ok(self
            .records
            .read()
            .expect("write audit repository lock poisoned")
            .iter()
            .filter(|r| &r.tag_id == tag_id)
            .cloned()
            .collect())
    }

    fn by_command_id(&self, command_id: &str) -> Result<Vec<WriteAuditRecord>, DomainError> {
        Ok(self
            .records
            .read()
            .expect("write audit repository lock poisoned")
            .iter()
            .filter(|r| r.command_id.as_deref() == Some(command_id))
            .cloned()
            .collect())
    }

    fn query(&self, query: &WriteAuditQuery) -> Result<Vec<WriteAuditRecord>, DomainError> {
        let mut filtered: Vec<WriteAuditRecord> = self
            .records
            .read()
            .expect("write audit repository lock poisoned")
            .iter()
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
            .cloned()
            .collect();

        if query.offset > 0 {
            filtered = filtered.into_iter().skip(query.offset).collect();
        }
        if let Some(limit) = query.limit {
            filtered.truncate(limit);
        }
        Ok(filtered)
    }
}
