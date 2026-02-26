//! Application layer for SCADA runtime/use-case orchestration.
//!
//! This crate owns runtime policies, command orchestration, and automation
//! evaluation. It is intentionally infrastructure-agnostic and delegates
//! transport/driver details to adapters in other crates.

pub mod automation;
pub mod runtime;

pub use automation::{ActionRequest, AutomationEngine, AutomationRuntimeScope};
pub use runtime::{
    ConnectionCommand, ConnectionRuntime, DeviceProtocolState, DriverFactory, DriverRegistry,
    EventBus, LiveState, PassthroughTagValuePipeline, RuntimeEngine, RuntimeEvent, TagRuntime,
    TagState, TagValuePipeline, WriteAuditQuery, WriteAuditRecord, WriteAuditRepository,
    WriteAuditStore, WriteCommandOutcome, WritePriority,
};
