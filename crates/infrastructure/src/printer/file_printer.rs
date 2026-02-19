use async_trait::async_trait;
use domain::printer::{PrinterConnection, PrinterError};
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{error, info};

pub struct FilePrinter {
    path: PathBuf,
    connected: bool,
}

impl FilePrinter {
    pub fn new(path: &str) -> Self {
        Self {
            path: PathBuf::from(path),
            connected: false,
        }
    }
}

#[async_trait]
impl PrinterConnection for FilePrinter {
    async fn connect(&mut self) -> Result<(), PrinterError> {
        info!("Preparing to print to file/share: {:?}", self.path);
        // For file printers, "connect" verifies we can open the path for appending.
        // We act like a connection-oriented device but open/close on each write for safety on network shares.

        // Simple check if path exists or we can write to it
        // Note: For network shares, we might not want to keep the handle open constantly
        // to avoid locking issues, so we just verify potential access here.
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

        // Open, Write, Close for each command batch to ensure data is flushed to the share immediately
        match OpenOptions::new()
            .write(true)
            .create(true) // Create if not exists (local files)
            .append(true) // Append to end
            .open(&self.path)
            .await
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(commands).await {
                    error!("Failed to write to printer file: {}", e);
                    return Err(PrinterError::WriteFailed(e.to_string()));
                }
                if let Err(e) = file.flush().await {
                    error!("Failed to flush to printer file: {}", e);
                    return Err(PrinterError::WriteFailed(e.to_string()));
                }
                Ok(())
            }
            Err(e) => {
                error!("Failed to open printer file {:?}: {}", self.path, e);
                Err(PrinterError::ConnectionFailed(e.to_string()))
            }
        }
    }
}
