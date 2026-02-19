use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PrinterError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Write failed: {0}")]
    WriteFailed(String),
    #[error("Not connected")]
    NotConnected,
}

#[async_trait]
pub trait PrinterConnection: Send + Sync {
    /// Attempt to establish a connection to the printer
    async fn connect(&mut self) -> Result<(), PrinterError>;

    /// Close the connection
    async fn disconnect(&mut self) -> Result<(), PrinterError>;

    /// Check if the connection is currently active
    async fn is_connected(&self) -> bool;

    /// Send raw bytes (ESC/POS commands) to the printer
    async fn send_commands(&mut self, commands: &[u8]) -> Result<(), PrinterError>;
}
