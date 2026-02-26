use async_trait::async_trait;
use dashmap::DashMap;
use domain::connection::{Connection, ConnectionRepository};
use domain::device::Device;
use domain::device_repository::DeviceRepository;
use domain::tag::{Tag, TagRepository};
use domain::id::{ConnectionId, DeviceId, TagId};
use domain::error::DomainError;

pub struct InMemoryConnectionRepository {
    connections: DashMap<ConnectionId, Connection>,
}

impl InMemoryConnectionRepository {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }
}

#[async_trait]
impl ConnectionRepository for InMemoryConnectionRepository {
    async fn find_by_id(&self, id: &ConnectionId) -> Result<Option<Connection>, DomainError> {
        Ok(self.connections.get(id).map(|r| r.value().clone()))
    }

    async fn find_all(&self) -> Result<Vec<Connection>, DomainError> {
        Ok(self.connections.iter().map(|r| r.value().clone()).collect())
    }

    async fn save(&self, connection: Connection) -> Result<(), DomainError> {
        self.connections.insert(connection.id.clone(), connection);
        Ok(())
    }

    async fn delete(&self, id: &ConnectionId) -> Result<(), DomainError> {
        self.connections.remove(id);
        Ok(())
    }
}

pub struct InMemoryDeviceRepository {
    devices: DashMap<DeviceId, Device>,
}

impl InMemoryDeviceRepository {
    pub fn new() -> Self {
        Self {
            devices: DashMap::new(),
        }
    }
}

#[async_trait]
impl DeviceRepository for InMemoryDeviceRepository {
    async fn find_by_id(&self, id: &DeviceId) -> Result<Option<Device>, DomainError> {
        Ok(self.devices.get(id).map(|r| r.value().clone()))
    }

    async fn find_all(&self) -> Result<Vec<Device>, DomainError> {
        Ok(self.devices.iter().map(|r| r.value().clone()).collect())
    }

    async fn save(&self, device: Device) -> Result<(), DomainError> {
        self.devices.insert(device.id.clone(), device);
        Ok(())
    }

    async fn delete(&self, id: &DeviceId) -> Result<(), DomainError> {
        self.devices.remove(id);
        Ok(())
    }
}

pub struct InMemoryTagRepository {
    tags: DashMap<TagId, Tag>,
}

impl InMemoryTagRepository {
    pub fn new() -> Self {
        Self {
            tags: DashMap::new(),
        }
    }
}

#[async_trait]
impl TagRepository for InMemoryTagRepository {
    async fn find_by_id(&self, id: &TagId) -> Result<Option<Tag>, DomainError> {
        Ok(self.tags.get(id).map(|r| r.value().clone()))
    }

    async fn find_all(&self) -> Result<Vec<Tag>, DomainError> {
        Ok(self.tags.iter().map(|r| r.value().clone()).collect())
    }

    async fn save(&self, tag: Tag) -> Result<(), DomainError> {
        self.tags.insert(tag.id.clone(), tag);
        Ok(())
    }

    async fn delete(&self, id: &TagId) -> Result<(), DomainError> {
        self.tags.remove(id);
        Ok(())
    }
}
