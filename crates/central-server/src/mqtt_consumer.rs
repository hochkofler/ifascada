use crate::ingestion::{IngestionError, IngestionService};
use crate::persistence::CentralPersistence;
use anyhow::Result;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// MQTT-level keep alive for the consumer's broker session.
const CONSUMER_KEEP_ALIVE: Duration = Duration::from_secs(20);

/// Multiplier applied to `keep_alive` to decide when a session is presumed dead.
///
/// `rumqttc` pings the broker every `keep_alive` regardless of traffic, so a live broker
/// must produce *some* inbound packet (at minimum a `PINGRESP`) within roughly that
/// period. Allowing 1.5x gives one whole keep alive period of slack before we declare the
/// session dead. This is the bound named in
/// `docs/finding-mqtt-client-stale-session-detection.md` (suggested fix #2).
const STALE_KEEP_ALIVE_MULTIPLIER: f64 = 1.5;

/// Floor for the watchdog's poll timeout so the loop can never busy-spin.
const MIN_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// Watchdog over "when did we last hear *from the broker*".
///
/// The failure mode this guards against (see the finding doc) is a session the broker has
/// dropped without a clean close, where the client keeps believing it is connected. In
/// that state `eventloop.poll()` can keep succeeding — it still emits
/// `Event::Outgoing(Outgoing::PingReq)` on every keep alive tick, because writing into a
/// half-open socket's send buffer does not fail — while nothing at all arrives from the
/// broker. Only *inbound* packets prove the broker is still there, so outgoing events
/// deliberately do not count as activity.
///
/// This is deliberately an *application-level backstop*, not the only line of defence.
/// `rumqttc` 0.24 has its own guard: `state.rs`'s `outgoing_ping()` returns
/// `StateError::AwaitPingResp` ("Last pingreq isn't acked") if the keep alive timer fires
/// again while a previous `PINGREQ` is still unanswered, which surfaces through `poll()` as
/// an `Err` and hits the reconnect path below. But that guard only bounds detection at
/// *two* keep alive periods (the ping timer has to fire twice), it lives entirely inside a
/// third-party state machine whose behaviour across versions is exactly what the
/// 2026-08-18 incident calls into question, and it cannot fire at all if the keep alive
/// timer branch never wins `rumqttc`'s internal `select!`. Owning the bound here makes it
/// explicit, testable, and tighter (1.5x rather than 2x).
///
/// Known limitation: a session where TCP and MQTT keep alive are both healthy but the
/// *subscription* is gone — the broker answers every `PINGREQ` yet delivers no publishes —
/// looks alive to this watchdog, because a `PINGRESP` is inbound activity. See the finding
/// doc for that separate hypothesis; it is not what this guards.
#[derive(Debug, Clone, Copy)]
struct BrokerActivityWatch {
    keep_alive: Duration,
    last_activity: Instant,
}

impl BrokerActivityWatch {
    fn new(keep_alive: Duration, now: Instant) -> Self {
        Self {
            keep_alive,
            last_activity: now,
        }
    }

    /// How long a silence from the broker we tolerate before presuming the session dead.
    fn stale_after(&self) -> Duration {
        self.keep_alive.mul_f64(STALE_KEEP_ALIVE_MULTIPLIER)
    }

    /// Record that something arrived *from the broker*, restarting the staleness window.
    fn record_activity(&mut self, now: Instant) {
        self.last_activity = now;
    }

    /// True once the broker has been silent for strictly longer than [`Self::stale_after`].
    fn should_force_reconnect(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_activity) > self.stale_after()
    }

    /// How long the poll loop may block before it must re-evaluate staleness.
    ///
    /// This is the timeout we wrap `eventloop.poll()` in: `poll()` can otherwise park
    /// indefinitely on a dead socket, which would keep the check below from ever running.
    /// It is deliberately the *remaining* time until the staleness deadline (never a fixed
    /// interval), so the timeout only ever expires at a point where we are going to tear
    /// the connection down anyway — that way cancelling `poll()` mid-flight can never
    /// leave a still-in-use connection in a half-written state.
    fn next_check_in(&self, now: Instant) -> Duration {
        self.stale_after()
            .saturating_sub(now.saturating_duration_since(self.last_activity))
            .max(MIN_CHECK_INTERVAL)
    }
}

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
        options.set_keep_alive(CONSUMER_KEEP_ALIVE);
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

        let mut watch = BrokerActivityWatch::new(CONSUMER_KEEP_ALIVE, Instant::now());

        loop {
            let now = Instant::now();
            if watch.should_force_reconnect(now) {
                warn!(
                    "no mqtt broker activity for {}s (> keep_alive*{}); presuming session dead, reconnect in {}s",
                    now.saturating_duration_since(watch.last_activity).as_secs(),
                    STALE_KEEP_ALIVE_MULTIPLIER,
                    backoff_secs
                );
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(30);
                break;
            }

            let event = match tokio::time::timeout(watch.next_check_in(now), eventloop.poll()).await
            {
                Ok(polled) => polled,
                Err(_elapsed) => {
                    // The staleness deadline arrived while `poll()` was still parked: the
                    // broker has not sent us anything (not even a PINGRESP) for
                    // `stale_after`. Drop this connection and rebuild it via the outer loop.
                    warn!(
                        "mqtt broker silent for {}s while polling (> keep_alive*{}); forcing reconnect in {}s",
                        watch.stale_after().as_secs(),
                        STALE_KEEP_ALIVE_MULTIPLIER,
                        backoff_secs
                    );
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(30);
                    break;
                }
            };

            // Only inbound packets prove the broker is still alive; `Event::Outgoing` is
            // produced by our own writes, which keep succeeding on a half-open socket.
            let heard_from_broker = matches!(&event, Ok(Event::Incoming(_)));

            match event {
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

            if heard_from_broker {
                // Stamped *after* the handler, not before it. The window this watchdog
                // measures is "silence observed at the socket", and we are not listening to
                // the socket while `ingest` is running: a 30s+ database stall would
                // otherwise be indistinguishable from a dead broker and would tear down a
                // perfectly healthy connection at exactly the worst moment.
                watch.record_activity(Instant::now());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keep_alive() -> Duration {
        Duration::from_secs(20)
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn a_fresh_watch_is_never_stale() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert!(!watch.should_force_reconnect(t0));
    }

    #[test]
    fn does_not_force_reconnect_while_broker_activity_is_recent() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert!(!watch.should_force_reconnect(t0 + secs(20)));
    }

    #[test]
    fn does_not_force_reconnect_exactly_at_the_1_5x_keep_alive_boundary() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        // keep_alive * 1.5 == 30s: at the boundary we still give the broker the benefit
        // of the doubt; only strictly beyond it do we declare the session dead.
        assert!(!watch.should_force_reconnect(t0 + secs(30)));
    }

    #[test]
    fn forces_reconnect_once_idle_exceeds_1_5x_keep_alive() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert!(watch.should_force_reconnect(t0 + secs(31)));
    }

    #[test]
    fn staleness_window_scales_with_keep_alive() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(secs(10), t0);
        assert_eq!(watch.stale_after(), secs(15));
        assert!(!watch.should_force_reconnect(t0 + secs(15)));
        assert!(watch.should_force_reconnect(t0 + secs(16)));
    }

    #[test]
    fn recorded_broker_activity_resets_the_staleness_window() {
        let t0 = Instant::now();
        let mut watch = BrokerActivityWatch::new(keep_alive(), t0);
        // e.g. a PingResp arriving 25s in: the session is alive, so the clock restarts.
        watch.record_activity(t0 + secs(25));
        assert!(!watch.should_force_reconnect(t0 + secs(50)));
        assert!(watch.should_force_reconnect(t0 + secs(56)));
    }

    #[test]
    fn a_clock_that_appears_to_move_backwards_never_forces_a_reconnect() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0 + secs(60));
        assert!(!watch.should_force_reconnect(t0));
    }

    #[test]
    fn next_check_in_wakes_the_loop_at_the_staleness_deadline() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert_eq!(watch.next_check_in(t0), secs(30));
        assert_eq!(watch.next_check_in(t0 + secs(25)), secs(5));
    }

    #[test]
    fn next_check_in_is_floored_so_the_poll_loop_cannot_spin() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert_eq!(watch.next_check_in(t0 + secs(30)), MIN_CHECK_INTERVAL);
        assert_eq!(watch.next_check_in(t0 + secs(600)), MIN_CHECK_INTERVAL);
    }

    #[test]
    fn the_first_staleness_check_cannot_cancel_an_in_flight_connect() {
        // rumqttc bounds its own CONNECT with NetworkOptions::connection_timeout (5s by
        // default). Our watchdog timeout wraps `poll()`, which performs that connect, so
        // the initial wait must be comfortably longer than it or we would cancel every
        // connect attempt and never come up.
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(CONSUMER_KEEP_ALIVE, t0);
        assert!(watch.next_check_in(t0) > secs(5));
    }
}

/// Live repro for the failure mode in
/// `docs/finding-mqtt-client-stale-session-detection.md` (its suggested fix #5 asks for
/// exactly this test): freeze a real broker mid-session so no FIN/RST ever reaches the
/// client, and assert the watchdog tears the connection down within `keep_alive * 1.5`.
///
/// `#[ignore]`d: it needs a throwaway broker and ~50s of wall clock, so it is opt-in.
/// Bring the broker up first (any port is fine as long as it matches `PORT` below):
///
/// ```text
/// docker run -d --name ifascada-stalerepro-mosquitto -p 51884:1883 \
///   -v "$PWD/docker/mosquitto/mosquitto.conf:/mosquitto/config/mosquitto.conf:ro" \
///   eclipse-mosquitto:2
/// cargo test -p central-server --lib live_repro -- --ignored --nocapture
/// docker rm -f ifascada-stalerepro-mosquitto
/// ```
///
/// Use a *throwaway* broker, never the shared dev stack's `ifascada-mosquitto`: the test
/// SIGSTOPs the container it targets.
#[cfg(test)]
mod live_repro {
    use super::*;
    use std::process::Command;

    const CONTAINER: &str = "ifascada-stalerepro-mosquitto";
    const PORT: u16 = 51884;

    /// `docker <action> <CONTAINER>`, returning false if docker or the container is absent.
    fn docker(action: &str) -> bool {
        match Command::new("docker").args([action, CONTAINER]).output() {
            Ok(out) => out.status.success(),
            Err(e) => {
                println!("docker unavailable ({e})");
                false
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn watchdog_detects_frozen_broker_within_1_5x_keep_alive() {
        // `unpause` doubles as the precondition check: it fails both when docker is absent
        // and when the container is not running, but also (harmlessly) when the container
        // is running and simply not paused — so only skip if the container is truly absent.
        docker("unpause");
        if !docker("inspect") {
            println!("SKIPPED: container '{CONTAINER}' not present (see this module's docs)");
            return;
        }

        let mut options = MqttOptions::new("stale-repro-watchdog", "127.0.0.1", PORT);
        options.set_keep_alive(CONSUMER_KEEP_ALIVE);
        options.set_clean_session(false);
        let (client, mut eventloop) = AsyncClient::new(options, 1024);
        client
            .subscribe("repro/#", QoS::AtLeastOnce)
            .await
            .expect("subscribe");

        let start = Instant::now();
        let mut watch = BrokerActivityWatch::new(CONSUMER_KEEP_ALIVE, Instant::now());
        let mut frozen_at: Option<Duration> = None;

        let detected_at = loop {
            let now = Instant::now();
            if watch.should_force_reconnect(now) {
                println!("  t+{:>5.1}s watchdog (loop top)", start.elapsed().as_secs_f64());
                break start.elapsed();
            }
            match tokio::time::timeout(watch.next_check_in(now), eventloop.poll()).await {
                Ok(Ok(ev)) => {
                    println!("  t+{:>5.1}s {:?}", start.elapsed().as_secs_f64(), ev);
                    if matches!(ev, Event::Incoming(_)) {
                        watch.record_activity(Instant::now());
                    }
                }
                Ok(Err(e)) => {
                    println!("  t+{:>5.1}s poll error: {e}", start.elapsed().as_secs_f64());
                    break start.elapsed();
                }
                Err(_) => {
                    println!("  t+{:>5.1}s watchdog (poll timeout)", start.elapsed().as_secs_f64());
                    break start.elapsed();
                }
            }
            // Freeze once the session is fully established (CONNACK + SUBACK are immediate).
            if frozen_at.is_none() && start.elapsed() > Duration::from_secs(3) {
                assert!(docker("pause"), "could not freeze {CONTAINER}");
                frozen_at = Some(start.elapsed());
                println!("  t+{:>5.1}s broker frozen (SIGSTOP: no FIN, no RST)", start.elapsed().as_secs_f64());
            }
        };

        docker("unpause");

        let since_freeze = detected_at - frozen_at.expect("broker was never frozen");
        println!("staleness detected {:.1}s after the freeze", since_freeze.as_secs_f64());
        // 35s, not 45s, is the meaningful bound: measured on rumqttc 0.24, plain
        // `poll()` alone only reports this same frozen broker after ~40s (two keep alive
        // periods, via its internal `AwaitPingResp` guard). Anything under 35s therefore
        // proves *our* watchdog fired, not the library's fallback.
        assert!(
            since_freeze < Duration::from_secs(35),
            "detection took {since_freeze:?}, expected < 35s"
        );
    }
}
