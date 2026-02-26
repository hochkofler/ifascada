use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use rand::RngCore;
use rumqttc::QoS;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use tracing::warn;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxMessageKind {
    Ack,
    Audit,
}

impl OutboxMessageKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ack => "ack",
            Self::Audit => "audit",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingMqttMessage {
    pub id: i64,
    pub topic: String,
    pub qos: u8,
    pub retain: bool,
    pub kind: OutboxMessageKind,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutboxStats {
    pub depth: usize,
    pub oldest_age_secs: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct OutboxKeyMaterial {
    pub key_id: String,
    pub encryption_key: Option<[u8; 32]>,
    pub hmac_key: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct OutboxSecurity {
    pub crypto_version: u16,
    pub active: OutboxKeyMaterial,
    pub previous: Vec<OutboxKeyMaterial>,
}

impl OutboxSecurity {
    pub fn from_rotation(
        active_key_id: String,
        active_encryption_secret: Option<String>,
        active_hmac_secret: Option<String>,
        previous_key_id: Option<String>,
        previous_encryption_secret: Option<String>,
        previous_hmac_secret: Option<String>,
    ) -> Self {
        let active = OutboxKeyMaterial {
            key_id: active_key_id,
            encryption_key: active_encryption_secret.map(|s| hash_to_32(s.as_bytes())),
            hmac_key: active_hmac_secret.map(|s| hash_to_32(s.as_bytes()).to_vec()),
        };
        let mut previous = Vec::new();
        if previous_encryption_secret.is_some() || previous_hmac_secret.is_some() {
            previous.push(OutboxKeyMaterial {
                key_id: previous_key_id.unwrap_or_else(|| "prev".to_string()),
                encryption_key: previous_encryption_secret.map(|s| hash_to_32(s.as_bytes())),
                hmac_key: previous_hmac_secret.map(|s| hash_to_32(s.as_bytes()).to_vec()),
            });
        }
        Self {
            crypto_version: 1,
            active,
            previous,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutboxConfig {
    pub max_messages: usize,
    pub security: OutboxSecurity,
}

impl Default for OutboxConfig {
    fn default() -> Self {
        Self {
            max_messages: 10_000,
            security: OutboxSecurity {
                crypto_version: 1,
                active: OutboxKeyMaterial {
                    key_id: "v1".to_string(),
                    encryption_key: None,
                    hmac_key: None,
                },
                previous: Vec::new(),
            },
        }
    }
}

#[async_trait]
pub trait OutboxPublisher: Send + Sync {
    async fn publish(
        &self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> Result<(), String>;
}

#[derive(Clone)]
pub struct PersistentMqttOutbox {
    conn: Arc<Mutex<Connection>>,
    cfg: OutboxConfig,
}

impl PersistentMqttOutbox {
    pub fn new(path: impl AsRef<Path>, cfg: OutboxConfig) -> anyhow::Result<Self> {
        let db_path: PathBuf = path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS mqtt_outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                topic TEXT NOT NULL,
                qos INTEGER NOT NULL,
                retain INTEGER NOT NULL,
                kind TEXT NOT NULL,
                payload BLOB NOT NULL,
                encrypted INTEGER NOT NULL,
                crypto_version INTEGER NOT NULL DEFAULT 1,
                key_id TEXT,
                nonce BLOB,
                signature BLOB,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_mqtt_outbox_id ON mqtt_outbox(id);
            CREATE INDEX IF NOT EXISTS idx_mqtt_outbox_kind ON mqtt_outbox(kind);",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            cfg,
        })
    }

    pub async fn enqueue(
        &self,
        kind: OutboxMessageKind,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.enforce_capacity(kind)?;

        let (encrypted, nonce, stored_payload) = self.protect_payload(&payload)?;
        let sig = self.sign(
            &self.cfg.security.active,
            &topic,
            qos_to_u8(qos),
            retain,
            kind,
            encrypted,
            nonce.as_deref(),
            &stored_payload,
        )?;

        let conn = self.conn.lock().expect("mqtt outbox connection lock poisoned");
        conn.execute(
            "INSERT INTO mqtt_outbox (topic, qos, retain, kind, payload, encrypted, crypto_version, key_id, nonce, signature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                topic,
                qos_to_u8(qos),
                if retain { 1 } else { 0 },
                kind.as_str(),
                stored_payload,
                if encrypted { 1 } else { 0 },
                self.cfg.security.crypto_version as i64,
                self.cfg.security.active.key_id.clone(),
                nonce,
                sig
            ],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn len(&self) -> usize {
        let conn = self.conn.lock().expect("mqtt outbox connection lock poisoned");
        conn.query_row("SELECT COUNT(*) FROM mqtt_outbox", [], |row| row.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }

    pub async fn stats(&self) -> OutboxStats {
        let conn = self.conn.lock().expect("mqtt outbox connection lock poisoned");
        let depth = conn
            .query_row("SELECT COUNT(*) FROM mqtt_outbox", [], |row| row.get::<_, i64>(0))
            .unwrap_or(0)
            .max(0) as usize;

        let oldest_age_secs = conn
            .query_row(
                "SELECT CASE WHEN COUNT(*) = 0 THEN NULL
                 ELSE CAST((strftime('%s','now') - strftime('%s', MIN(created_at))) AS INTEGER) END
                 FROM mqtt_outbox",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .ok()
            .flatten()
            .map(|v| v.max(0) as u64);

        OutboxStats {
            depth,
            oldest_age_secs,
        }
    }

    pub async fn flush_pending<P: OutboxPublisher>(
        &self,
        publisher: &P,
        max_batch: usize,
    ) -> anyhow::Result<usize> {
        let pending = self.load_pending(max_batch)?;
        let mut sent = 0usize;
        for raw in pending {
            let msg = match self.unprotect_row(&raw) {
                Ok(v) => v,
                Err(e) => {
                    warn!("dropping corrupt outbox row {}: {}", raw.id, e);
                    self.delete_by_id(raw.id)?;
                    continue;
                }
            };

            let qos = u8_to_qos(msg.qos);
            match publisher
                .publish(msg.topic.clone(), qos, msg.retain, msg.payload.clone())
                .await
            {
                Ok(_) => {
                    self.delete_by_id(msg.id)?;
                    sent += 1;
                }
                Err(e) => {
                    warn!("mqtt outbox flush failed, will retry later: {}", e);
                    break;
                }
            }
        }
        Ok(sent)
    }

    fn enforce_capacity(&self, incoming_kind: OutboxMessageKind) -> anyhow::Result<()> {
        let count = self.current_count()?;
        if count < self.cfg.max_messages {
            return Ok(());
        }

        match incoming_kind {
            OutboxMessageKind::Ack => {
                let deleted = self.delete_oldest_by_kind(OutboxMessageKind::Audit)?;
                if deleted == 0 {
                    self.delete_oldest_any()?;
                }
            }
            OutboxMessageKind::Audit => {
                let deleted = self.delete_oldest_by_kind(OutboxMessageKind::Audit)?;
                if deleted == 0 {
                    warn!("outbox full, dropping incoming audit message to preserve ACK priority");
                    return Err(anyhow::anyhow!("outbox at capacity for audit"));
                }
            }
        }
        Ok(())
    }

    fn current_count(&self) -> anyhow::Result<usize> {
        let conn = self.conn.lock().expect("mqtt outbox connection lock poisoned");
        let c = conn.query_row("SELECT COUNT(*) FROM mqtt_outbox", [], |r| r.get::<_, i64>(0))?;
        Ok(c as usize)
    }

    fn delete_oldest_by_kind(&self, kind: OutboxMessageKind) -> anyhow::Result<usize> {
        let conn = self.conn.lock().expect("mqtt outbox connection lock poisoned");
        let affected = conn.execute(
            "DELETE FROM mqtt_outbox WHERE id = (
                SELECT id FROM mqtt_outbox WHERE kind = ?1 ORDER BY id ASC LIMIT 1
            )",
            params![kind.as_str()],
        )?;
        Ok(affected)
    }

    fn delete_oldest_any(&self) -> anyhow::Result<usize> {
        let conn = self.conn.lock().expect("mqtt outbox connection lock poisoned");
        let affected = conn.execute(
            "DELETE FROM mqtt_outbox WHERE id = (
                SELECT id FROM mqtt_outbox ORDER BY id ASC LIMIT 1
            )",
            [],
        )?;
        Ok(affected)
    }

    fn load_pending(&self, max_batch: usize) -> anyhow::Result<Vec<RawPendingRow>> {
        let conn = self.conn.lock().expect("mqtt outbox connection lock poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, topic, qos, retain, kind, payload, encrypted, crypto_version, key_id, nonce, signature
             FROM mqtt_outbox ORDER BY id ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![max_batch as i64], |row| {
            Ok(RawPendingRow {
                id: row.get(0)?,
                topic: row.get(1)?,
                qos: row.get(2)?,
                retain: row.get::<_, i64>(3)? != 0,
                kind: parse_kind(&row.get::<_, String>(4)?),
                payload: row.get(5)?,
                encrypted: row.get::<_, i64>(6)? != 0,
                crypto_version: row.get::<_, i64>(7)? as u16,
                key_id: row.get(8)?,
                nonce: row.get(9)?,
                signature: row.get(10)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn delete_by_id(&self, id: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("mqtt outbox connection lock poisoned");
        conn.execute("DELETE FROM mqtt_outbox WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn protect_payload(&self, plain: &[u8]) -> anyhow::Result<(bool, Option<Vec<u8>>, Vec<u8>)> {
        if let Some(key) = self.cfg.security.active.encryption_key {
            let cipher = Aes256Gcm::new_from_slice(&key)?;
            let mut nonce_bytes = [0u8; 12];
            rand::thread_rng().fill_bytes(&mut nonce_bytes);
            let nonce = Nonce::from_slice(&nonce_bytes);
            let encrypted = cipher
                .encrypt(nonce, plain)
                .map_err(|e| anyhow::anyhow!("encrypt payload failed: {}", e))?;
            Ok((true, Some(nonce_bytes.to_vec()), encrypted))
        } else {
            Ok((false, None, plain.to_vec()))
        }
    }

    fn unprotect_row(&self, raw: &RawPendingRow) -> anyhow::Result<PendingMqttMessage> {
        if raw.crypto_version > self.cfg.security.crypto_version {
            return Err(anyhow::anyhow!(
                "unsupported crypto_version {}",
                raw.crypto_version
            ));
        }
        let material = self.select_key_material(raw.key_id.as_deref());
        self.verify(
            material,
            &raw.topic,
            raw.qos,
            raw.retain,
            raw.kind,
            raw.encrypted,
            raw.nonce.as_deref(),
            &raw.payload,
            raw.signature.as_deref(),
        )?;

        let payload = if raw.encrypted {
            let key = material
                .and_then(|m| m.encryption_key)
                .ok_or_else(|| anyhow::anyhow!("row is encrypted but no key configured"))?;
            let nonce = raw
                .nonce
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("encrypted row missing nonce"))?;
            if nonce.len() != 12 {
                return Err(anyhow::anyhow!("invalid nonce length"));
            }
            let cipher = Aes256Gcm::new_from_slice(&key)?;
            let dec = cipher
                .decrypt(Nonce::from_slice(nonce), raw.payload.as_ref())
                .map_err(|e| anyhow::anyhow!("decrypt payload failed: {}", e))?;
            dec
        } else {
            raw.payload.clone()
        };

        Ok(PendingMqttMessage {
            id: raw.id,
            topic: raw.topic.clone(),
            qos: raw.qos,
            retain: raw.retain,
            kind: raw.kind,
            payload,
        })
    }

    fn sign(
        &self,
        material: &OutboxKeyMaterial,
        topic: &str,
        qos: u8,
        retain: bool,
        kind: OutboxMessageKind,
        encrypted: bool,
        nonce: Option<&[u8]>,
        payload: &[u8],
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let Some(key) = material.hmac_key.as_ref() else {
            return Ok(None);
        };
        let mut mac = <HmacSha256 as Mac>::new_from_slice(key)?;
        mac.update(topic.as_bytes());
        mac.update(&[qos]);
        mac.update(&[if retain { 1 } else { 0 }]);
        mac.update(kind.as_str().as_bytes());
        mac.update(&[if encrypted { 1 } else { 0 }]);
        if let Some(n) = nonce {
            mac.update(n);
        }
        mac.update(payload);
        Ok(Some(mac.finalize().into_bytes().to_vec()))
    }

    fn verify(
        &self,
        material: Option<&OutboxKeyMaterial>,
        topic: &str,
        qos: u8,
        retain: bool,
        kind: OutboxMessageKind,
        encrypted: bool,
        nonce: Option<&[u8]>,
        payload: &[u8],
        signature: Option<&[u8]>,
    ) -> anyhow::Result<()> {
        let Some(key) = material.and_then(|m| m.hmac_key.as_ref()) else {
            return Ok(());
        };
        let sig = signature.ok_or_else(|| anyhow::anyhow!("missing hmac signature"))?;
        let mut mac = <HmacSha256 as Mac>::new_from_slice(key)?;
        mac.update(topic.as_bytes());
        mac.update(&[qos]);
        mac.update(&[if retain { 1 } else { 0 }]);
        mac.update(kind.as_str().as_bytes());
        mac.update(&[if encrypted { 1 } else { 0 }]);
        if let Some(n) = nonce {
            mac.update(n);
        }
        mac.update(payload);
        mac.verify_slice(sig)
            .map_err(|_| anyhow::anyhow!("hmac verification failed"))
    }

    fn select_key_material(&self, key_id: Option<&str>) -> Option<&OutboxKeyMaterial> {
        if let Some(id) = key_id {
            if self.cfg.security.active.key_id == id {
                return Some(&self.cfg.security.active);
            }
            return self.cfg.security.previous.iter().find(|k| k.key_id == id);
        }
        Some(&self.cfg.security.active)
    }
}

#[derive(Debug, Clone)]
struct RawPendingRow {
    id: i64,
    topic: String,
    qos: u8,
    retain: bool,
    kind: OutboxMessageKind,
    payload: Vec<u8>,
    encrypted: bool,
    crypto_version: u16,
    key_id: Option<String>,
    nonce: Option<Vec<u8>>,
    signature: Option<Vec<u8>>,
}

fn parse_kind(s: &str) -> OutboxMessageKind {
    match s {
        "ack" => OutboxMessageKind::Ack,
        _ => OutboxMessageKind::Audit,
    }
}

fn qos_to_u8(qos: QoS) -> u8 {
    match qos {
        QoS::AtMostOnce => 0,
        QoS::AtLeastOnce => 1,
        QoS::ExactlyOnce => 2,
    }
}

fn u8_to_qos(v: u8) -> QoS {
    match v {
        2 => QoS::ExactlyOnce,
        1 => QoS::AtLeastOnce,
        _ => QoS::AtMostOnce,
    }
}

fn hash_to_32(input: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(input);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::Mutex as TokioMutex;

    struct FakePublisher {
        fail_all: bool,
        sent: TokioMutex<Vec<Vec<u8>>>,
    }

    #[async_trait]
    impl OutboxPublisher for FakePublisher {
        async fn publish(
            &self,
            _topic: String,
            _qos: QoS,
            _retain: bool,
            payload: Vec<u8>,
        ) -> Result<(), String> {
            if self.fail_all {
                Err("simulated publish error".to_string())
            } else {
                self.sent.lock().await.push(payload);
                Ok(())
            }
        }
    }

    fn temp_file(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}.db", name, stamp))
    }

    #[tokio::test]
    async fn test_outbox_enqueue_persists_and_reloads() {
        let path = temp_file("mqtt_outbox");
        let outbox = PersistentMqttOutbox::new(&path, OutboxConfig::default()).unwrap();
        outbox
            .enqueue(
                OutboxMessageKind::Audit,
                "topic/a".to_string(),
                QoS::AtLeastOnce,
                false,
                b"hello".to_vec(),
            )
            .await
            .unwrap();
        let reloaded = PersistentMqttOutbox::new(&path, OutboxConfig::default()).unwrap();
        assert_eq!(reloaded.len().await, 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_outbox_flush_success_drains_queue() {
        let path = temp_file("mqtt_outbox_flush_ok");
        let outbox = PersistentMqttOutbox::new(&path, OutboxConfig::default()).unwrap();
        outbox
            .enqueue(
                OutboxMessageKind::Audit,
                "topic/a".to_string(),
                QoS::AtLeastOnce,
                false,
                b"one".to_vec(),
            )
            .await
            .unwrap();
        outbox
            .enqueue(
                OutboxMessageKind::Ack,
                "topic/b".to_string(),
                QoS::AtLeastOnce,
                false,
                b"two".to_vec(),
            )
            .await
            .unwrap();

        let pubr = FakePublisher {
            fail_all: false,
            sent: TokioMutex::new(Vec::new()),
        };
        let sent = outbox.flush_pending(&pubr, 10).await.unwrap();
        assert_eq!(sent, 2);
        assert_eq!(outbox.len().await, 0);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_outbox_flush_failure_keeps_queue() {
        let path = temp_file("mqtt_outbox_flush_fail");
        let outbox = PersistentMqttOutbox::new(&path, OutboxConfig::default()).unwrap();
        outbox
            .enqueue(
                OutboxMessageKind::Audit,
                "topic/a".to_string(),
                QoS::AtLeastOnce,
                false,
                b"one".to_vec(),
            )
            .await
            .unwrap();

        let sent = outbox
            .flush_pending(
                &FakePublisher {
                    fail_all: true,
                    sent: TokioMutex::new(Vec::new()),
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(sent, 0);
        assert_eq!(outbox.len().await, 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_outbox_capacity_preserves_ack_priority() {
        let path = temp_file("mqtt_outbox_capacity");
        let outbox = PersistentMqttOutbox::new(
            &path,
            OutboxConfig {
                max_messages: 1,
                ..OutboxConfig::default()
            },
        )
        .unwrap();

        outbox
            .enqueue(
                OutboxMessageKind::Ack,
                "topic/ack".to_string(),
                QoS::AtLeastOnce,
                false,
                b"ack".to_vec(),
            )
            .await
            .unwrap();

        let res = outbox
            .enqueue(
                OutboxMessageKind::Audit,
                "topic/audit".to_string(),
                QoS::AtLeastOnce,
                false,
                b"audit".to_vec(),
            )
            .await;
        assert!(res.is_err());
        assert_eq!(outbox.len().await, 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_outbox_security_encrypts_and_signs_roundtrip() {
        let path = temp_file("mqtt_outbox_security");
        let outbox = PersistentMqttOutbox::new(
            &path,
            OutboxConfig {
                max_messages: 100,
                security: OutboxSecurity::from_rotation(
                    "k1".to_string(),
                    Some("enc-secret".to_string()),
                    Some("hmac-secret".to_string()),
                    None,
                    None,
                    None,
                ),
            },
        )
        .unwrap();

        outbox
            .enqueue(
                OutboxMessageKind::Audit,
                "topic/secure".to_string(),
                QoS::AtLeastOnce,
                false,
                b"sensitive".to_vec(),
            )
            .await
            .unwrap();

        let pubr = FakePublisher {
            fail_all: false,
            sent: TokioMutex::new(Vec::new()),
        };
        let sent = outbox.flush_pending(&pubr, 10).await.unwrap();
        assert_eq!(sent, 1);
        let delivered = pubr.sent.lock().await;
        assert_eq!(delivered[0], b"sensitive".to_vec());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_outbox_key_rotation_reads_old_messages_with_previous_key() {
        let path = temp_file("mqtt_outbox_rotation");
        let old_cfg = OutboxConfig {
            max_messages: 100,
            security: OutboxSecurity::from_rotation(
                "k-old".to_string(),
                Some("enc-old".to_string()),
                Some("hmac-old".to_string()),
                None,
                None,
                None,
            ),
        };
        let outbox_old = PersistentMqttOutbox::new(&path, old_cfg).unwrap();
        outbox_old
            .enqueue(
                OutboxMessageKind::Audit,
                "topic/rotate".to_string(),
                QoS::AtLeastOnce,
                false,
                b"old-payload".to_vec(),
            )
            .await
            .unwrap();

        let new_cfg = OutboxConfig {
            max_messages: 100,
            security: OutboxSecurity::from_rotation(
                "k-new".to_string(),
                Some("enc-new".to_string()),
                Some("hmac-new".to_string()),
                Some("k-old".to_string()),
                Some("enc-old".to_string()),
                Some("hmac-old".to_string()),
            ),
        };
        let outbox_new = PersistentMqttOutbox::new(&path, new_cfg).unwrap();
        let pubr = FakePublisher {
            fail_all: false,
            sent: TokioMutex::new(Vec::new()),
        };
        let sent = outbox_new.flush_pending(&pubr, 10).await.unwrap();
        assert_eq!(sent, 1);
        let delivered = pubr.sent.lock().await;
        assert_eq!(delivered[0], b"old-payload".to_vec());
        let _ = std::fs::remove_file(path);
    }
}
