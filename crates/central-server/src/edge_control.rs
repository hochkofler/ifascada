//! The long-poll at the heart of the out-of-band control channel.
//!
//! A supervisor leaves a request here and it resolves as soon as an order appears, or
//! comes back empty when the wait expires. See
//! `docs/superpowers/specs/2026-09-02-edge-out-of-band-control-design.md`.

use anyhow::Result;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PendingOrder {
    pub request_id: String,
    pub kind: String,
}

/// One `Notify` per edge, created on demand. Waking only the supervisor that cares keeps a
/// busy plant from stampeding every long-poll on every insert.
#[derive(Clone, Default)]
pub struct EdgeWaiters {
    inner: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

impl EdgeWaiters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_edge(&self, edge_code: &str) -> Arc<Notify> {
        let mut map = self.inner.lock().expect("edge waiters lock poisoned");
        map.entry(edge_code.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    /// Called after an order is inserted, so a waiting supervisor hears about it at once.
    pub fn wake(&self, edge_code: &str) {
        self.for_edge(edge_code).notify_waiters();
    }
}

/// Resolves as soon as `fetch` reports an order, when `notify` fires, or when `wait`
/// expires -- whichever comes first. The database is always consulted again before
/// answering, so a lost notification costs latency and never correctness.
pub async fn wait_for_order<F, Fut>(
    fetch: F,
    notify: &Notify,
    wait: Duration,
) -> Result<Option<PendingOrder>>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<Option<PendingOrder>>>,
{
    // Register for the wake *before* the first read. An order inserted in the gap between
    // reading and waiting would otherwise notify nobody, and this request would sit idle
    // for the whole window with its answer already in the table.
    let notified = notify.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();

    if let Some(order) = fetch().await? {
        return Ok(Some(order));
    }

    // A wake and an expiry lead to the same place: read the table again. That is what
    // keeps a lost notification a latency problem rather than a correctness one.
    let _ = tokio::time::timeout(wait, notified).await;

    fetch().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    fn order(id: &str) -> PendingOrder {
        PendingOrder {
            request_id: id.to_string(),
            kind: "restart".to_string(),
        }
    }

    /// A supervisor that arrives after the operator pressed the button must not be made to
    /// wait out the full window.
    #[tokio::test]
    async fn an_order_already_queued_comes_back_without_waiting() {
        let notify = Notify::new();
        let started = Instant::now();

        let got = wait_for_order(
            || async { Ok(Some(order("req-1"))) },
            &notify,
            Duration::from_secs(25),
        )
        .await
        .unwrap();

        assert_eq!(got, Some(order("req-1")));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "an order already in the table must not wait"
        );
    }

    #[tokio::test]
    async fn a_notification_wakes_the_wait_and_the_table_is_read_again() {
        let notify = Arc::new(Notify::new());
        let calls = Arc::new(AtomicUsize::new(0));

        let waker = notify.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            waker.notify_waiters();
        });

        let seen = calls.clone();
        let got = wait_for_order(
            move || {
                let seen = seen.clone();
                async move {
                    // Empty the first time, as it would be before the operator acted.
                    if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(order("req-2")))
                    }
                }
            },
            &notify,
            Duration::from_secs(25),
        )
        .await
        .unwrap();

        assert_eq!(got, Some(order("req-2")));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "the table must be read again after the wake, not trusted from the notification"
        );
    }

    /// The property the whole design rests on: the notification is an optimisation, never
    /// the source of truth. With no notification at all the wait still expires and reads
    /// the table, so a central restart or a missed wake costs latency and nothing else.
    #[tokio::test]
    async fn the_order_is_found_on_expiry_even_when_no_notification_ever_arrives() {
        let notify = Notify::new();
        let calls = Arc::new(AtomicUsize::new(0));

        let seen = calls.clone();
        let got = wait_for_order(
            move || {
                let seen = seen.clone();
                async move {
                    if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(order("req-3")))
                    }
                }
            },
            &notify,
            Duration::from_millis(200),
        )
        .await
        .unwrap();

        assert_eq!(
            got,
            Some(order("req-3")),
            "expiry must re-read the table; correctness cannot depend on the notification"
        );
    }

    #[tokio::test]
    async fn a_quiet_window_answers_empty() {
        let notify = Notify::new();
        let got = wait_for_order(|| async { Ok(None) }, &notify, Duration::from_millis(150))
            .await
            .unwrap();
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn a_database_failure_is_reported_rather_than_answered_as_quiet() {
        let notify = Notify::new();
        let got = wait_for_order(
            || async { Err(anyhow::anyhow!("connection reset")) },
            &notify,
            Duration::from_millis(150),
        )
        .await;

        assert!(
            got.is_err(),
            "a failed read must not look like 'nothing to do'"
        );
    }

    #[tokio::test]
    async fn waking_one_edge_does_not_wake_another() {
        let waiters = EdgeWaiters::new();
        let lcc01 = waiters.for_edge("lcc01");
        let lcc02 = waiters.for_edge("lcc02");

        let woken = Arc::new(AtomicUsize::new(0));
        let counter = woken.clone();
        let watcher = tokio::spawn(async move {
            lcc02.notified().await;
            counter.fetch_add(1, Ordering::SeqCst);
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        waiters.wake("lcc01");
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(
            woken.load(Ordering::SeqCst),
            0,
            "lcc02's supervisor must not be woken by lcc01's order"
        );

        // And the right edge does get through, so the test cannot pass by nobody waking.
        waiters.wake("lcc02");
        tokio::time::timeout(Duration::from_secs(2), watcher)
            .await
            .expect("lcc02 was never woken by its own order")
            .unwrap();
        assert_eq!(woken.load(Ordering::SeqCst), 1);

        let _ = lcc01;
    }
}
