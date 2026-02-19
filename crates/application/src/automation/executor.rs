use async_trait::async_trait;
use domain::automation::ActionConfig;
use domain::tag::TagId;
use tracing::{debug, info};

use crate::printer::batch_manager::BatchManager;
use crate::printer::builder::ReceiptBuilder;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

#[async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(&self, action: &ActionConfig, tag_id: &TagId, payload: &serde_json::Value);
    async fn execute_manual_batch(&self, tag_id: &TagId, items: Vec<ReportItem>);
}

pub struct LoggingActionExecutor;

#[async_trait]
impl ActionExecutor for LoggingActionExecutor {
    async fn execute(&self, action: &ActionConfig, tag_id: &TagId, payload: &serde_json::Value) {
        match action {
            ActionConfig::PrintTicket {
                template,
                service_url: _,
            } => {
                info!(tag_id = %tag_id, template = %template, "🖨️ [LOG] PRINT ACTION TRIGGERED");
                debug!("Payload: {:?}", payload);
            }
            ActionConfig::PublishMqtt {
                topic,
                payload_template,
            } => {
                info!(topic = %topic, template = %payload_template, "📡 [LOG] MQTT PUBLISH TRIGGERED");
            }
            ActionConfig::AccumulateData {
                session_id,
                template,
            } => {
                info!(session = %session_id, template = %template, "📦 [LOG] ACCUMULATE DATA");
            }
            ActionConfig::PrintBatch { session_id, .. } => {
                info!(session = %session_id, "🖨️ [LOG] PRINT BATCH");
            }
        }
    }

    async fn execute_manual_batch(&self, tag_id: &TagId, items: Vec<ReportItem>) {
        info!(tag_id = %tag_id, count = %items.len(), "🖨️ [LOG] MANUAL BATCH PRINT TRIGGERED");
    }
}

use domain::event::{DomainEvent, EventPublisher, ReportItem};

pub struct PrintingActionExecutor {
    print_queue: mpsc::Sender<Vec<u8>>,
    // Map of SessionID -> BatchManager
    batch_managers: Arc<Mutex<HashMap<String, BatchManager>>>,
    agent_id: String,
    publisher: Arc<dyn EventPublisher>,
}

impl PrintingActionExecutor {
    pub fn new(
        print_queue: mpsc::Sender<Vec<u8>>,
        agent_id: String,
        publisher: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            print_queue,
            batch_managers: Arc::new(Mutex::new(HashMap::new())),
            agent_id,
            publisher,
        }
    }

    async fn send_job(&self, data: Vec<u8>) {
        if let Err(e) = self.print_queue.send(data).await {
            tracing::error!("Failed to enqueue print job: {}", e);
        } else {
            info!("✅ Print job enqueued");
        }
    }

    async fn process_batch_print(&self, tag_id: &TagId, items: Vec<ReportItem>, header: &str) {
        if items.is_empty() {
            tracing::warn!(tag_id=%tag_id, "⚠️ Batch items empty, skipping print.");
            return;
        }

        // 1. Publish Report Event (for Traceability)
        let unique_report_id = format!("man_{}_{}", tag_id, uuid::Uuid::new_v4());
        let event = DomainEvent::report_completed(
            unique_report_id.clone(),
            self.agent_id.clone(),
            items.clone(),
        );

        if let Err(e) = self.publisher.publish(event).await {
            tracing::error!(report_id=%unique_report_id, tag_id=%tag_id, error=%e, "❌ Failed to publish report event");
        } else {
            tracing::info!(report_id=%unique_report_id, tag_id=%tag_id, "📤 Report event published");
        }

        // 2. Build Physical Batch Ticket
        let mut builder = ReceiptBuilder::new()
            .initialize()
            .align_center()
            .text_line(header)
            .separator()
            .align_left();

        for (i, item) in items.iter().enumerate() {
            // For now, simple decimal or string representation of JSON value
            let val_str = match &item.value {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Object(map) => map
                    .get("value")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| item.value.to_string()),
                _ => item.value.to_string(),
            };
            let line = format!("{}. {:>8}", i + 1, val_str);
            builder = builder.text_line(&line);
        }

        let receipt = builder
            .separator()
            .align_center()
            .text_line("FIN DEL REPORTE")
            .feed(2)
            .cut()
            .build();

        self.send_job(receipt).await;
    }
}

#[async_trait]
impl ActionExecutor for PrintingActionExecutor {
    async fn execute(&self, action: &ActionConfig, tag_id: &TagId, payload: &serde_json::Value) {
        match action {
            ActionConfig::PrintTicket { template, .. } => {
                info!(tag_id = %tag_id, template = %template, "🖨️ Generating Unit Ticket...");

                let val_str = extract_value(payload);

                let receipt = ReceiptBuilder::new()
                    .initialize()
                    .align_center()
                    .text_line("LABORATORIOS IFA S.A.")
                    .separator()
                    .align_left()
                    .kv("Tag:", tag_id.as_str())
                    .kv("Valor:", &val_str)
                    .kv(
                        "Fecha:",
                        &chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    )
                    .separator()
                    .feed(2)
                    .cut()
                    .build();

                self.send_job(receipt).await;
            }
            ActionConfig::AccumulateData {
                session_id,
                template: _,
            } => {
                let session_id = session_id.trim();
                info!(session=%session_id, "📦 Accumulating data into manager...");

                let mut managers = self.batch_managers.lock().await;
                let manager = managers
                    .entry(session_id.to_string())
                    .or_insert_with(BatchManager::new);
                manager.add_item(payload.clone(), None);
            }
            ActionConfig::PrintBatch {
                session_id,
                header_template,
                footer_template: _,
            } => {
                let session_id = session_id.trim();
                info!(session=%session_id, "🖨️ Printing Batch...");

                let mut managers = self.batch_managers.lock().await;
                if let Some(manager) = managers.get_mut(session_id) {
                    let items_raw = manager.take_batch();
                    let items: Vec<ReportItem> = items_raw
                        .into_iter()
                        .map(|i| ReportItem {
                            value: i.value,
                            timestamp: i.timestamp,
                            metadata: i.metadata,
                        })
                        .collect();

                    self.process_batch_print(tag_id, items, header_template)
                        .await;
                } else {
                    tracing::warn!(session=%session_id, total_sessions=%managers.len(), "⚠️ No batch session found");
                }
            }
            ActionConfig::PublishMqtt { .. } => {
                tracing::warn!("MQTT Action not yet implemented in PrintingExecutor");
            }
        }
    }

    async fn execute_manual_batch(&self, tag_id: &TagId, items: Vec<ReportItem>) {
        info!(tag_id = %tag_id, count = %items.len(), "🖨️ Generating Manual Batch Ticket...");
        self.process_batch_print(tag_id, items, "REPORTE MANUAL DE PESAJES")
            .await;
    }
}

fn extract_value(payload: &serde_json::Value) -> String {
    match payload {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Object(map) => {
            if let Some(v) = map.get("value") {
                v.to_string()
            } else {
                payload.to_string()
            }
        }
        _ => payload.to_string(),
    }
}
