//! Infrastructure layer - External integrations

pub mod config;
pub mod database;
pub mod drivers;
pub mod messaging;
pub mod pipeline;
pub mod printer;
pub mod repositories;

pub use database::{
    PostgresEventPublisher, PostgresTagRepository, SeaOrmDeviceRepository, SeaOrmTagRepository,
};
pub use drivers::DriverFactory;
pub use messaging::buffered_publisher::BufferedMqttPublisher;
pub use messaging::composite_publisher::CompositeEventPublisher;
pub use messaging::mqtt_client::{MqttClient, MqttMessage};
pub use messaging::mqtt_publisher::MqttEventPublisher;
