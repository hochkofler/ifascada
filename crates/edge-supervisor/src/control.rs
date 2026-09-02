//! The out-of-band control channel: a long-poll against central that carries restart
//! orders to a supervisor whose agent may be wedged.
//!
//! Deliberately not MQTT and deliberately not a persistent connection. See
//! `docs/superpowers/specs/2026-09-02-edge-out-of-band-control-design.md`.

use crate::config::ControlConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Slack added on top of central's own wait before the client gives up, so a normal empty
/// reply is never mistaken for a dead connection.
pub const CLIENT_TIMEOUT_MARGIN: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    /// Central had nothing to say within its wait. Ask again.
    None,
    Restart { request_id: String },
}

#[derive(Debug, Serialize)]
struct PendingRequest<'a> {
    edge_id: &'a str,
    enrollment_token: &'a str,
}

#[derive(Debug, Serialize)]
struct AckRequest<'a> {
    edge_id: &'a str,
    enrollment_token: &'a str,
    request_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct PendingResponse {
    /// Absent or null when there is no order.
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
}

pub struct ControlClient {
    http: reqwest::Client,
    cfg: ControlConfig,
}

impl ControlClient {
    pub fn new(cfg: ControlConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(cfg.wait + CLIENT_TIMEOUT_MARGIN)
            .build()
            .context("failed to build the control http client")?;
        Ok(ControlClient { http, cfg })
    }

    /// The client's own timeout. Must exceed central's wait or every quiet period would
    /// look like a network failure.
    pub fn request_timeout(&self) -> Duration {
        self.cfg.wait + CLIENT_TIMEOUT_MARGIN
    }

    /// Leaves a request with central that resolves as soon as there is an order, or comes
    /// back empty when central's wait expires. An empty answer is the normal quiet case.
    pub async fn wait_for_order(&self) -> Result<Order> {
        let url = format!("{}/api/edge/control/pending", self.cfg.base_url);
        let resp = self
            .http
            .post(&url)
            .json(&PendingRequest {
                edge_id: &self.cfg.edge_id,
                enrollment_token: &self.cfg.enroll_token,
            })
            .send()
            .await
            .context("control long-poll failed")?;

        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("central answered {} to the control poll", status);
        }

        let body: PendingResponse = resp
            .json()
            .await
            .context("could not read central's control answer")?;

        match (body.kind.as_deref(), body.request_id) {
            (None, _) => Ok(Order::None),
            (Some("restart"), Some(request_id)) => Ok(Order::Restart { request_id }),
            // A malformed or unknown order is not worth stopping the supervisor over, and
            // must not be acked -- staying quiet leaves it pending and visible in central.
            (Some(kind), id) => {
                tracing::warn!(
                    "ignoring unusable control order kind={:?} request_id={:?}",
                    kind,
                    id
                );
                Ok(Order::None)
            }
        }
    }

    /// Confirms an order was carried out, so central stops handing it back.
    pub async fn ack(&self, request_id: &str) -> Result<()> {
        let url = format!("{}/api/edge/control/ack", self.cfg.base_url);
        let resp = self
            .http
            .post(&url)
            .json(&AckRequest {
                edge_id: &self.cfg.edge_id,
                enrollment_token: &self.cfg.enroll_token,
                request_id,
            })
            .send()
            .await
            .context("control ack failed")?;

        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("central answered {} to the control ack", status);
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod fake_central {
    //! A stand-in for central that the control tests drive. Kept in one place so every
    //! scenario is expressed the same way and new ones are cheap to add.

    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Reply {
        /// Answer immediately with an order.
        Order { request_id: String },
        /// Answer immediately with "nothing", the way a timed-out long-poll does.
        Empty,
        /// Hold the request open for this long, then answer empty.
        HoldThenEmpty(std::time::Duration),
        /// Answer with an HTTP error status.
        Status(u16),
        /// Hand over an order, then refuse to accept the acknowledgement. Separates
        /// "the restart happened" from "central knows it happened".
        OrderThenFailingAck { request_id: String },
    }

    #[derive(Debug, Default)]
    pub struct Seen {
        pub pending_bodies: Vec<serde_json::Value>,
        pub ack_bodies: Vec<serde_json::Value>,
    }

    pub struct FakeCentral {
        pub base_url: String,
        pub seen: Arc<Mutex<Seen>>,
    }

    /// Starts a fake central on an ephemeral port. The task is left running for the life
    /// of the test process; tests are short and this keeps the helper simple.
    pub async fn start(reply: Reply) -> FakeCentral {
        use axum::extract::State;
        use axum::http::StatusCode;
        use axum::routing::post;
        use axum::{Json, Router};

        #[derive(Clone)]
        struct Ctx {
            reply: Reply,
            seen: Arc<Mutex<Seen>>,
        }

        async fn pending(
            State(ctx): State<Ctx>,
            Json(body): Json<serde_json::Value>,
        ) -> (StatusCode, Json<serde_json::Value>) {
            ctx.seen.lock().unwrap().pending_bodies.push(body);
            match &ctx.reply {
                Reply::Order { request_id } => (
                    StatusCode::OK,
                    Json(serde_json::json!({ "kind": "restart", "request_id": request_id })),
                ),
                Reply::Empty => (StatusCode::OK, Json(serde_json::json!({}))),
                Reply::HoldThenEmpty(d) => {
                    tokio::time::sleep(*d).await;
                    (StatusCode::OK, Json(serde_json::json!({})))
                }
                Reply::Status(code) => (
                    StatusCode::from_u16(*code).unwrap(),
                    Json(serde_json::json!({})),
                ),
                Reply::OrderThenFailingAck { request_id } => (
                    StatusCode::OK,
                    Json(serde_json::json!({ "kind": "restart", "request_id": request_id })),
                ),
            }
        }

        async fn ack(
            State(ctx): State<Ctx>,
            Json(body): Json<serde_json::Value>,
        ) -> StatusCode {
            ctx.seen.lock().unwrap().ack_bodies.push(body);
            match ctx.reply {
                Reply::Status(code) => StatusCode::from_u16(code).unwrap(),
                Reply::OrderThenFailingAck { .. } => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::OK,
            }
        }

        let seen = Arc::new(Mutex::new(Seen::default()));
        let ctx = Ctx {
            reply,
            seen: seen.clone(),
        };
        let app = Router::new()
            .route("/api/edge/control/pending", post(pending))
            .route("/api/edge/control/ack", post(ack))
            .with_state(ctx);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        FakeCentral {
            base_url: format!("http://{}", addr),
            seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake_central::{self, Reply};
    use super::*;

    fn client_for(base_url: &str, wait_secs: u64) -> ControlClient {
        ControlClient::new(ControlConfig {
            base_url: base_url.to_string(),
            enroll_token: "s3cret".to_string(),
            edge_id: "lcc01".to_string(),
            wait: Duration::from_secs(wait_secs),
        })
        .unwrap()
    }

    #[tokio::test]
    async fn an_order_waiting_at_central_comes_back_as_a_restart() {
        let central = fake_central::start(Reply::Order {
            request_id: "req-42".to_string(),
        })
        .await;
        let client = client_for(&central.base_url, 25);

        let order = client.wait_for_order().await.expect("request failed");

        assert_eq!(
            order,
            Order::Restart {
                request_id: "req-42".to_string()
            }
        );
    }

    #[tokio::test]
    async fn the_request_identifies_and_authenticates_the_edge() {
        let central = fake_central::start(Reply::Empty).await;
        let client = client_for(&central.base_url, 25);

        client.wait_for_order().await.expect("request failed");

        let seen = central.seen.lock().unwrap();
        let body = seen.pending_bodies.first().expect("central saw no request");
        assert_eq!(body["edge_id"], "lcc01");
        assert_eq!(body["enrollment_token"], "s3cret");
    }

    /// A long-poll that comes back empty is the normal quiet case, not a failure. If this
    /// surfaced as an error the supervisor would log an anomaly every 25 seconds forever.
    #[tokio::test]
    async fn a_quiet_period_is_not_an_error() {
        let central = fake_central::start(Reply::Empty).await;
        let client = client_for(&central.base_url, 25);

        assert_eq!(client.wait_for_order().await.unwrap(), Order::None);
    }

    /// The whole point of long-poll: central holds the request and the client waits it out
    /// rather than treating the silence as a dead connection.
    #[tokio::test]
    async fn the_client_waits_out_a_held_request() {
        let central =
            fake_central::start(Reply::HoldThenEmpty(Duration::from_millis(600))).await;
        // A one-second wait means a client timeout of 11s, comfortably longer than the hold.
        let client = client_for(&central.base_url, 1);

        let order = client
            .wait_for_order()
            .await
            .expect("a held request must not be treated as a failure");
        assert_eq!(order, Order::None);
    }

    /// Guards the invariant that makes the above work. Were the client timeout ever set
    /// below central's wait, every quiet period would surface as a network error.
    #[test]
    fn the_client_timeout_outlasts_centrals_wait() {
        let client = client_for("http://127.0.0.1:1", 25);
        assert!(
            client.request_timeout() > Duration::from_secs(25),
            "the client must outwait central, otherwise silence looks like failure"
        );
    }

    #[tokio::test]
    async fn a_server_error_is_reported_so_the_caller_can_back_off() {
        let central = fake_central::start(Reply::Status(500)).await;
        let client = client_for(&central.base_url, 25);

        assert!(client.wait_for_order().await.is_err());
    }

    #[tokio::test]
    async fn a_rejected_token_is_reported() {
        let central = fake_central::start(Reply::Status(401)).await;
        let client = client_for(&central.base_url, 25);

        assert!(client.wait_for_order().await.is_err());
    }

    /// Central being down is the expected state during its own restarts and deployments.
    #[tokio::test]
    async fn an_unreachable_central_is_reported_not_panicked_on() {
        // Port 1 is reserved and nothing listens there.
        let client = client_for("http://127.0.0.1:1", 1);
        assert!(client.wait_for_order().await.is_err());
    }

    #[tokio::test]
    async fn an_ack_names_the_order_it_confirms() {
        let central = fake_central::start(Reply::Empty).await;
        let client = client_for(&central.base_url, 25);

        client.ack("req-42").await.expect("ack failed");

        let seen = central.seen.lock().unwrap();
        let body = seen.ack_bodies.first().expect("central saw no ack");
        assert_eq!(body["edge_id"], "lcc01");
        assert_eq!(body["enrollment_token"], "s3cret");
        assert_eq!(body["request_id"], "req-42");
    }

    #[tokio::test]
    async fn a_failed_ack_is_reported() {
        let central = fake_central::start(Reply::Status(503)).await;
        let client = client_for(&central.base_url, 25);

        assert!(client.ack("req-42").await.is_err());
    }
}
