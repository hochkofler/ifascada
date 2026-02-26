pub mod entity;
pub mod repository;
pub mod state;

pub use entity::{Connection, ConnectionConfig, ReconnectStrategy, ReconnectionPolicy};
pub use repository::ConnectionRepository;
pub use state::{
    ConnectionTransition, classify_connection_transition, normalize_connection_state,
};
