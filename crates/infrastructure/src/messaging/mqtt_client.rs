use anyhow::{Result, anyhow};
use rumqttc::{AsyncClient, Event, LastWill, MqttOptions, Packet, QoS};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task;
use tracing::{error, info};

#[derive(Clone, Debug)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: Vec<u8>,
    pub pkid: u16,
}

#[async_trait::async_trait]
pub trait MqttPublisherClient: Send + Sync {
    async fn publish_bytes(
        &self,
        topic: &str,
        payload: &[u8],
        qos: QoS,
        retain: bool,
    ) -> Result<()>;
    fn is_connected(&self) -> bool;
}

#[derive(Clone)]
pub struct MqttClient {
    client: AsyncClient,
    tx: broadcast::Sender<MqttMessage>,
    connected: Arc<AtomicBool>,
    subscriptions: Arc<std::sync::RwLock<Vec<String>>>,
}

impl MqttClient {
    pub async fn new(
        host: &str,
        port: u16,
        client_id: &str,
        last_will: Option<LastWill>,
    ) -> Result<Self> {
        let mut mqttoptions = MqttOptions::new(client_id, host, port);
        mqttoptions.set_keep_alive(Duration::from_secs(20));
        mqttoptions.set_clean_session(false); // Persistent session for commands
        mqttoptions.set_manual_acks(true); // Enable Manual Acks for reliability

        if let Some(will) = last_will {
            mqttoptions.set_last_will(will);
        }

        let (client, mut eventloop) = AsyncClient::new(mqttoptions, 100);
        let (tx, _) = broadcast::channel(250);
        let tx_clone = tx.clone();
        let connected = Arc::new(AtomicBool::new(false));
        let connected_clone = connected.clone();

        let subscriptions = Arc::new(std::sync::RwLock::new(Vec::new()));
        let subscriptions_clone = subscriptions.clone();
        let client_clone = client.clone();

        // Spawn a task to handle the event loop
        task::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(notification) => match notification {
                        Event::Incoming(Packet::Publish(publish)) => {
                            let msg = MqttMessage {
                                topic: publish.topic,
                                payload: publish.payload.to_vec(),
                                pkid: publish.pkid,
                            };
                            if let Err(tokio::sync::broadcast::error::SendError(returned_msg)) =
                                tx_clone.send(msg)
                            {
                                // Ignore send errors (happens when no one is listening yet)
                                // to avoid spamming "channel closed" during startup.
                                if returned_msg.topic.contains("config") {
                                    tracing::warn!(
                                        "⚠️ Dropped MQTT message for topic '{}' because no internal subscribers are listening yet.",
                                        returned_msg.topic
                                    );
                                }
                            } else {
                                // We can't access msg here because it was moved into send
                                // And publish.topic was moved into msg
                                // So we can't log "Looped message" efficiently without cloning
                                // Let's skip the success log for now to avoid clone overhead on every packet
                            }
                        }
                        Event::Incoming(Packet::ConnAck(_)) => {
                            info!("MQTT Connected");
                            connected_clone.store(true, Ordering::Relaxed);

                            // Re-subscribe to all topics
                            let subs = subscriptions_clone.read().unwrap().clone();
                            if !subs.is_empty() {
                                info!("Re-subscribing to {} topics...", subs.len());
                                for topic in subs {
                                    if let Err(e) =
                                        client_clone.subscribe(&topic, QoS::AtLeastOnce).await
                                    {
                                        error!("Failed to re-subscribe to {}: {}", topic, e);
                                    }
                                }
                            }
                        }
                        Event::Outgoing(rumqttc::Outgoing::Disconnect) => {
                            connected_clone.store(false, Ordering::Relaxed);
                        }
                        _ => {}
                    },
                    Err(e) => {
                        error!("MQTT Connection error: {:?}", e);
                        connected_clone.store(false, Ordering::Relaxed);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(Self {
            client,
            tx,
            connected,
            subscriptions,
        })
    }

    pub fn subscribe_messages(&self) -> broadcast::Receiver<MqttMessage> {
        self.tx.subscribe()
    }

    pub async fn publish(&self, topic: &str, payload: &str, retain: bool) -> Result<()> {
        self.publish_bytes(topic, payload.as_bytes(), rumqttc::QoS::AtLeastOnce, retain)
            .await
    }

    pub async fn subscribe(&self, topic: &str) -> Result<()> {
        {
            let mut subs = self.subscriptions.write().unwrap();
            if !subs.contains(&topic.to_string()) {
                subs.push(topic.to_string());
            }
        }

        self.client
            .subscribe(topic, QoS::AtLeastOnce)
            .await
            .map_err(|e| anyhow!("Failed to subscribe to topic {}: {}", topic, e))?;
        Ok(())
    }

    pub async fn ack(&self, topic: &str, pkid: u16) -> Result<()> {
        let publish = rumqttc::Publish {
            pkid,
            topic: topic.to_string(),
            qos: rumqttc::QoS::AtLeastOnce,
            payload: bytes::Bytes::new(),
            retain: false,
            dup: false,
        };

        self.client
            .ack(&publish)
            .await
            .map_err(|e| anyhow!("Failed to ack packet {}: {}", pkid, e))
    }
}

#[async_trait::async_trait]
impl MqttPublisherClient for MqttClient {
    async fn publish_bytes(
        &self,
        topic: &str,
        payload: &[u8],
        qos: QoS,
        retain: bool,
    ) -> Result<()> {
        self.client
            .publish(topic, qos, retain, payload)
            .await
            .map_err(|e| anyhow!("Failed to publish MQTT message: {}", e))?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}
