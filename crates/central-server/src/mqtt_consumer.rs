use crate::ingestion::{IngestionError, IngestionService};
use crate::persistence::CentralPersistence;
use anyhow::Result;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::time::Duration;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct MqttConsumerConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub topic_filters: Vec<String>,
    pub clean_session: bool,
    pub manual_acks: bool,
}

pub async fn run_mqtt_consumer<P: CentralPersistence + 'static>(
    cfg: MqttConsumerConfig,
    ingestion: IngestionService<P>,
) -> Result<()> {
    let mut backoff_secs = 1u64;
    loop {
        debug!(
            "mqtt consumer connect attempt host={} port={} client_id={} clean_session={} manual_acks={}",
            cfg.host, cfg.port, cfg.client_id, cfg.clean_session, cfg.manual_acks
        );
        let mut options = MqttOptions::new(cfg.client_id.clone(), cfg.host.clone(), cfg.port);
        options.set_keep_alive(Duration::from_secs(20));
        options.set_clean_session(cfg.clean_session);
        options.set_manual_acks(cfg.manual_acks);

        let (client, mut eventloop) = AsyncClient::new(options, 1024);
        let mut subscribe_failed = false;
        for topic_filter in &cfg.topic_filters {
            if let Err(e) = client.subscribe(topic_filter.clone(), QoS::AtLeastOnce).await {
                warn!(
                    "failed to subscribe mqtt topic='{}': {}; retry in {}s",
                    topic_filter, e, backoff_secs
                );
                subscribe_failed = true;
                break;
            }
        }
        if subscribe_failed {
            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(30);
            continue;
        }
        info!(
            "central-server subscribed to topics {:?} (clean_session={}, manual_acks={})",
            cfg.topic_filters, cfg.clean_session, cfg.manual_acks
        );
        backoff_secs = 1;

        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::Publish(p))) => {
                    debug!(
                        "mqtt incoming publish topic='{}' qos={:?} retain={} bytes={}",
                        p.topic,
                        p.qos,
                        p.retain,
                        p.payload.len()
                    );
                    let ingest_res = ingestion.ingest(&p.topic, p.payload.as_ref()).await;
                    let should_ack = match &ingest_res {
                        Ok(_) => true,
                        Err(IngestionError::NonRetryable(e)) => {
                            warn!("non-retryable ingest failure topic='{}': {}", p.topic, e);
                            true
                        }
                        Err(IngestionError::Retryable(e)) => {
                            warn!("retryable ingest failure topic='{}': {}", p.topic, e);
                            false
                        }
                    };
                    if cfg.manual_acks {
                        if should_ack {
                            if let Err(e) = client.ack(&p).await {
                                warn!("failed to ack topic='{}': {}", p.topic, e);
                            } else {
                                debug!("mqtt ack sent topic='{}'", p.topic);
                            }
                        } else {
                            debug!("mqtt ack skipped topic='{}' (retryable ingest failure)", p.topic);
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(
                        "mqtt consumer event loop error: {}; reconnect in {}s",
                        e, backoff_secs
                    );
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(30);
                    break;
                }
            }
        }
    }
}
