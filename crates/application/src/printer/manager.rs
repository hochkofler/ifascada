use domain::printer::PrinterConnection;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};

pub struct PrinterManager {
    connection: Box<dyn PrinterConnection>,
    job_rx: mpsc::Receiver<Vec<u8>>,
    reconnect_interval: Duration,
}

impl PrinterManager {
    pub fn new(connection: Box<dyn PrinterConnection>, job_rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            connection,
            job_rx,
            reconnect_interval: Duration::from_secs(5),
        }
    }

    pub async fn run(mut self) {
        info!("🖨️ Printer Manager started");

        // Initial connection attempt
        self.connect_loop().await;

        loop {
            tokio::select! {
                // Handle new print jobs
                Some(job) = self.job_rx.recv() => {
                    if self.connection.is_connected().await {
                         match self.connection.send_commands(&job).await {
                             Ok(_) => info!("✅ Print job sent ({} bytes)", job.len()),
                             Err(e) => {
                                 error!("❌ Failed to print: {}. Reconnecting...", e);
                                 self.connect_loop().await;

                                 // Retry logic: Try once more after reconnect
                                 if self.connection.is_connected().await {
                                     if let Err(e2) = self.connection.send_commands(&job).await {
                                         error!("❌ Retry failed: {}. Job dropped.", e2);
                                     } else {
                                         info!("✅ Retry success");
                                     }
                                 }
                             }
                         }
                    } else {
                        warn!("⚠️ Printer disconnected. Dropping job ({} bytes). attempting to reconnect...", job.len());
                         self.connect_loop().await;
                    }
                }
                else => {
                    // All senders dropped — printer channel closed, exit loop gracefully
                    info!("🖨️ Printer job channel closed. PrinterManager shutting down.");
                    break;
                }
            }
        }
    }

    async fn connect_loop(&mut self) {
        // Double check strict connection status
        if self.connection.is_connected().await {
            return;
        }

        warn!("🔌 Connecting to printer...");
        loop {
            match self.connection.connect().await {
                Ok(_) => {
                    info!("✅ Printer connected");
                    break;
                }
                Err(e) => {
                    error!(
                        "❌ Connection failed: {}. Retrying in {:?}...",
                        e, self.reconnect_interval
                    );
                    sleep(self.reconnect_interval).await;
                }
            }
        }
    }
}
