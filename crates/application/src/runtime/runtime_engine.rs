use crate::runtime::connection_runtime::{ConnectionCommand, ConnectionRuntime, WritePriority};
use crate::runtime::driver_registry::DriverRegistry;
use crate::runtime::event_bus::{EventBus, RuntimeEvent, WriteCommandOutcome};
use crate::runtime::live_state::LiveState;
use crate::runtime::tag_runtime::TagRuntime;
use crate::runtime::write_audit::{
    WriteAuditQuery, WriteAuditRecord, WriteAuditRepository, WriteAuditStore,
};
use tracing::warn;
use domain::connection::Connection;
use domain::id::{ConnectionId, TagId};
use domain::tag::TagValue;
use domain::tag::Tag;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub struct RuntimeEngine {
    event_bus: EventBus,
    live_state: Arc<LiveState>,
    driver_registry: Arc<DriverRegistry>,
    connections: HashMap<ConnectionId, mpsc::Sender<ConnectionCommand>>,
    tag_routes: HashMap<TagId, ConnectionId>,
    write_audit_repository: Arc<dyn WriteAuditRepository>,
}

impl RuntimeEngine {
    pub fn new(driver_registry: DriverRegistry) -> Self {
        Self::new_with_write_audit_repository(driver_registry, Arc::new(WriteAuditStore::new()))
    }

    pub fn new_with_write_audit_repository(
        driver_registry: DriverRegistry,
        write_audit_repository: Arc<dyn WriteAuditRepository>,
    ) -> Self {
        Self {
            event_bus: EventBus::new(1024),
            live_state: Arc::new(LiveState::new()),
            driver_registry: Arc::new(driver_registry),
            connections: HashMap::new(),
            tag_routes: HashMap::new(),
            write_audit_repository,
        }
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub fn live_state(&self) -> Arc<LiveState> {
        self.live_state.clone()
    }

    pub fn write_audit_all(&self) -> Result<Vec<WriteAuditRecord>, domain::DomainError> {
        self.write_audit_repository.all()
    }

    pub fn write_audit_by_tag(
        &self,
        tag_id: &TagId,
    ) -> Result<Vec<WriteAuditRecord>, domain::DomainError> {
        self.write_audit_repository.by_tag(tag_id)
    }

    pub fn write_audit_by_command_id(
        &self,
        command_id: &str,
    ) -> Result<Vec<WriteAuditRecord>, domain::DomainError> {
        self.write_audit_repository.by_command_id(command_id)
    }

    pub fn write_audit_query(
        &self,
        query: &WriteAuditQuery,
    ) -> Result<Vec<WriteAuditRecord>, domain::DomainError> {
        self.write_audit_repository.query(query)
    }

    pub async fn start_connection(
        &mut self,
        connection: Connection,
        tags: Vec<Tag>,
    ) -> anyhow::Result<()> {
        let conn_id = connection.id.clone();

        if self.connections.contains_key(&conn_id) {
            warn!(
                "start_connection ignored: connection '{}' already running; use reload_connection to apply changes",
                conn_id
            );
            return Ok(());
        }

        let driver = self
            .driver_registry
            .create_driver(&connection)
            .ok_or_else(|| {
                anyhow::anyhow!("Unsupported driver type: {}", connection.config.driver_type)
            })?;

        for tag in &tags {
            self.tag_routes.insert(tag.id.clone(), conn_id.clone());
        }

        let tag_runtimes = tags.into_iter().map(TagRuntime::new).collect();
        let (tx, rx) = mpsc::channel(32);

        let runtime = ConnectionRuntime::new(
            connection,
            driver,
            tag_runtimes,
            self.event_bus.clone(),
            self.live_state.clone(),
            self.write_audit_repository.clone(),
            rx,
        );

        tokio::spawn(runtime.run());
        self.connections.insert(conn_id, tx);

        Ok(())
    }

    pub async fn stop_connection(&mut self, id: &ConnectionId) {
        if let Some(tx) = self.connections.remove(id) {
            let _ = tx.send(ConnectionCommand::Stop).await;
        }
        self.tag_routes.retain(|_, conn_id| conn_id != id);
    }

    pub async fn reload_connection(&mut self, connection: Connection) -> anyhow::Result<()> {
        if let Some(tx) = self.connections.get(&connection.id) {
            tx.send(ConnectionCommand::Reload(connection))
                .await
                .map_err(|e| anyhow::anyhow!("failed to send reload command: {}", e))?;
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "connection not running: {}",
                connection.id
            ))
        }
    }

    pub async fn stop_all(&mut self) {
        let ids: Vec<ConnectionId> = self.connections.keys().cloned().collect();
        for id in ids {
            self.stop_connection(&id).await;
        }
        self.tag_routes.clear();
    }

    pub async fn write_tag(
        &self,
        tag_id: TagId,
        value: TagValue,
    ) -> Result<(), domain::DomainError> {
        self.write_tag_with_optional_command_id(tag_id, value, None, WritePriority::Normal)
            .await
    }

    pub async fn write_tag_with_command_id(
        &self,
        tag_id: TagId,
        value: TagValue,
        command_id: String,
    ) -> Result<(), domain::DomainError> {
        self.write_tag_with_optional_command_id(
            tag_id,
            value,
            Some(command_id),
            WritePriority::Normal,
        )
            .await
    }

    pub async fn write_tag_with_command_id_and_priority(
        &self,
        tag_id: TagId,
        value: TagValue,
        command_id: String,
        priority: WritePriority,
    ) -> Result<(), domain::DomainError> {
        self.write_tag_with_optional_command_id(tag_id, value, Some(command_id), priority)
            .await
    }

    pub async fn write_tag_with_priority(
        &self,
        tag_id: TagId,
        value: TagValue,
        priority: WritePriority,
    ) -> Result<(), domain::DomainError> {
        self.write_tag_with_optional_command_id(tag_id, value, None, priority)
            .await
    }

    async fn write_tag_with_optional_command_id(
        &self,
        tag_id: TagId,
        value: TagValue,
        command_id: Option<String>,
        priority: WritePriority,
    ) -> Result<(), domain::DomainError> {
        let conn_id = if let Some(conn_id) = self.tag_routes.get(&tag_id) {
            conn_id
        } else {
            let err = domain::DomainError::NotFound(format!("tag route not found: {}", tag_id));
            self.publish_write_rejected(None, &tag_id, &value, command_id.clone(), &err);
            return Err(err);
        };

        let tx = if let Some(tx) = self.connections.get(conn_id) {
            tx
        } else {
            let err = domain::DomainError::NotFound(format!("connection not running: {}", conn_id));
            self.publish_write_rejected(Some(conn_id.clone()), &tag_id, &value, command_id.clone(), &err);
            return Err(err);
        };

        let send_tag_id = tag_id.clone();
        let send_value = value.clone();
        let send_command_id = command_id.clone();
        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send(ConnectionCommand::WriteTag {
            tag_id: send_tag_id.clone(),
            value: send_value.clone(),
            command_id: send_command_id.clone(),
            priority,
            respond_to: resp_tx,
        })
        .await
        .map_err(|e| {
            let err = domain::DomainError::DriverError(format!("failed to dispatch write: {}", e));
            self.publish_write_rejected(
                Some(conn_id.clone()),
                &send_tag_id,
                &send_value,
                send_command_id.clone(),
                &err,
            );
            err
        })?;

        resp_rx.await.map_err(|e| {
            let err = domain::DomainError::DriverError(format!("write response channel closed: {}", e));
            self.publish_write_rejected(
                Some(conn_id.clone()),
                &send_tag_id,
                &send_value,
                send_command_id.clone(),
                &err,
            );
            err
        })?
    }

    fn publish_write_rejected(
        &self,
        connection_id: Option<ConnectionId>,
        tag_id: &TagId,
        value: &TagValue,
        command_id: Option<String>,
        err: &domain::DomainError,
    ) {
        let timestamp = chrono::Utc::now();
        let reason = Some(err.to_string());
        self.event_bus.publish(RuntimeEvent::TagWriteCommandHandled {
            connection_id: connection_id.clone(),
            tag_id: tag_id.clone(),
            command_id: command_id.clone(),
            value: value.clone(),
            outcome: WriteCommandOutcome::Rejected,
            reason: reason.clone(),
            timestamp,
        });
        if let Err(e) = self.write_audit_repository.append(WriteAuditRecord {
            connection_id,
            tag_id: tag_id.clone(),
            command_id,
            value: value.clone(),
            outcome: WriteCommandOutcome::Rejected,
            reason,
            timestamp,
        }) {
            warn!("failed to persist write audit record: {}", e);
        }
    }
}
