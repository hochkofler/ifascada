use async_trait::async_trait;
use domain::printer::{PrinterConnection, PrinterError};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct MockPrinter {
    pub connected: bool,
    pub sent_data: Arc<Mutex<Vec<u8>>>,
}

impl MockPrinter {
    pub fn new() -> Self {
        Self {
            connected: false,
            sent_data: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl PrinterConnection for MockPrinter {
    async fn connect(&mut self) -> Result<(), PrinterError> {
        self.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), PrinterError> {
        self.connected = false;
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.connected
    }

    async fn send_commands(&mut self, commands: &[u8]) -> Result<(), PrinterError> {
        if !self.connected {
            return Err(PrinterError::NotConnected);
        }
        let mut data = self.sent_data.lock().await;
        data.extend_from_slice(commands);
        Ok(())
    }
}
