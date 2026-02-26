// Core modules already created
pub mod automation;
pub mod connection;
pub mod device;
pub mod device_repository;
pub mod driver;
pub mod error;
pub mod id;
pub mod tag;

pub use connection::{
    Connection, ConnectionConfig, ConnectionRepository, ConnectionTransition,
    classify_connection_transition, normalize_connection_state,
};
pub use automation::{
    ActionSpec, AutomationEvalResult, AutomationEvalState, AutomationEvent, AutomationSpec,
    ExecutionScope, NumericOperator, TriggerSpec, evaluate_automation,
};
pub use device::{Device, DeviceConnectionStatus, DeviceStatusInput, evaluate_device_connection_status};
pub use device_repository::DeviceRepository;
pub use driver::DriverType;
pub use error::DomainError;
pub use id::{ConnectionId, DeviceId, TagId};
pub use tag::{
    QualityReason, QualityStatus, Tag, TagConnectionStatus, TagQuality, TagRepository,
    TagStatusInput, TagUpdateMode, TagValue, TagValueType, evaluate_tag_connection_status,
};
