use crate::database::entities::devices;
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use domain::DomainError;
use domain::device::{Device, DeviceRepository};
use domain::driver::DriverType;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set};

pub struct SeaOrmDeviceRepository {
    db: DatabaseConnection,
    agent_id: String,
}

impl SeaOrmDeviceRepository {
    pub fn new(db: DatabaseConnection, agent_id: String) -> Self {
        Self { db, agent_id }
    }

    fn model_to_device(&self, model: devices::Model) -> Result<Device, DomainError> {
        let driver_type = match model.driver_type.as_str() {
            "RS232" => DriverType::RS232,
            "Simulator" => DriverType::Simulator,
            "Modbus" => DriverType::Modbus,
            "OPC-UA" => DriverType::OPCUA,
            "HTTP" => DriverType::HTTP,
            // Fallback or error?
            // If unknown, maybe error? Or default to Simulator for safety?
            // Let's error.
            _ => {
                return Err(DomainError::InvalidConfiguration(format!(
                    "Unknown driver type: {}",
                    model.driver_type
                )));
            }
        };

        Ok(Device::new(
            model.id,
            driver_type,
            model.connection_config,
            model.enabled,
        ))
    }

    fn to_offset(dt: DateTime<Utc>) -> DateTime<FixedOffset> {
        dt.with_timezone(&FixedOffset::east_opt(0).unwrap())
    }
}

#[async_trait]
impl DeviceRepository for SeaOrmDeviceRepository {
    async fn save(&self, device: &Device) -> Result<(), DomainError> {
        let now = Utc::now();
        let now_offset = Self::to_offset(now);

        let driver_type_str = match device.driver {
            DriverType::RS232 => "RS232",
            DriverType::Simulator => "Simulator",
            DriverType::Modbus => "Modbus",
            DriverType::OPCUA => "OPC-UA",
            DriverType::HTTP => "HTTP",
        };

        let active_model = devices::ActiveModel {
            id: Set(device.id.clone()),
            edge_agent_id: Set(self.agent_id.clone()),
            name: Set(device.id.clone()), // Use ID as name if name missing in Device struct
            driver_type: Set(driver_type_str.to_string()),
            connection_config: Set(device.connection_config.clone()),
            enabled: Set(device.enabled),
            created_at: Set(now_offset),
            updated_at: Set(now_offset),
        };

        devices::Entity::insert(active_model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(devices::Column::Id)
                    .update_columns([
                        devices::Column::DriverType,
                        devices::Column::ConnectionConfig,
                        devices::Column::Enabled,
                        devices::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Device>, DomainError> {
        let model = devices::Entity::find_by_id(id.to_string())
            .one(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        match model {
            Some(m) => Ok(Some(self.model_to_device(m)?)),
            None => Ok(None),
        }
    }

    async fn find_all(&self) -> Result<Vec<Device>, DomainError> {
        let models = devices::Entity::find()
            .order_by_asc(devices::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        let mut result = Vec::new();
        for m in models {
            result.push(self.model_to_device(m)?);
        }
        Ok(result)
    }

    async fn find_by_agent(&self, agent_id: &str) -> Result<Vec<Device>, DomainError> {
        let models = devices::Entity::find()
            .filter(devices::Column::EdgeAgentId.eq(agent_id))
            .order_by_asc(devices::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;

        let mut result = Vec::new();
        for m in models {
            result.push(self.model_to_device(m)?);
        }
        Ok(result)
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        devices::Entity::delete_by_id(id.to_string())
            .exec(&self.db)
            .await
            .map_err(|e| DomainError::InvalidConfiguration(format!("Database error: {}", e)))?;
        Ok(())
    }
}
