use async_trait::async_trait;
use domain::printer::{PrinterConnection, PrinterError};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::{error, info};

pub struct NetworkPrinter {
    address: String,
    stream: Option<TcpStream>,
    timeout: Duration,
}

impl NetworkPrinter {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            address: format!("{}:{}", host, port),
            stream: None,
            timeout: Duration::from_secs(5),
        }
    }
}

#[async_trait]
impl PrinterConnection for NetworkPrinter {
    async fn connect(&mut self) -> Result<(), PrinterError> {
        info!("Connecting to printer at {}", self.address);
        match tokio::time::timeout(self.timeout, TcpStream::connect(&self.address)).await {
            Ok(Ok(stream)) => {
                info!("Connected to printer!");
                self.stream = Some(stream);
                Ok(())
            }
            Ok(Err(e)) => Err(PrinterError::ConnectionFailed(e.to_string())),
            Err(_) => Err(PrinterError::ConnectionFailed(
                "Connection timed out".to_string(),
            )),
        }
    }

    async fn disconnect(&mut self) -> Result<(), PrinterError> {
        if let Some(mut stream) = self.stream.take() {
            let _ = stream.shutdown().await;
        }
        Ok(())
    }

    async fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    async fn send_commands(&mut self, commands: &[u8]) -> Result<(), PrinterError> {
        if let Some(stream) = &mut self.stream {
            match stream.write_all(commands).await {
                Ok(_) => {
                    let _ = stream.flush().await;
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to write to printer: {}", e);
                    self.stream = None; // Invalidate connection
                    Err(PrinterError::WriteFailed(e.to_string()))
                }
            }
        } else {
            Err(PrinterError::NotConnected)
        }
    }
}
