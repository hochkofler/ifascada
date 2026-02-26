use application::runtime::{
    DriverFactory, DriverRegistry, RuntimeEngine, WriteAuditQuery, WriteAuditRepository,
    WriteCommandOutcome, WritePriority,
};
use chrono::{Duration as ChronoDuration, Utc};
use async_trait::async_trait;
use domain::connection::Connection;
use domain::connection::{ReconnectStrategy, ReconnectionPolicy};
use domain::driver::{ConnectionState, DriverConnection};
use domain::DriverType;
use domain::id::{ConnectionId, DeviceId, TagId};
use domain::tag::{Tag, TagUpdateMode, TagValue};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::VecDeque;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{Duration, sleep};

struct MockDriver {
    state: ConnectionState,
}

#[async_trait]
impl DriverConnection for MockDriver {
    async fn connect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Connected;
        Ok(())
    }
    async fn disconnect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Disconnected;
        Ok(())
    }
    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }
    fn state(&self) -> ConnectionState {
        self.state
    }
    async fn poll(
        &mut self,
    ) -> Result<
        Vec<(TagId, Result<TagValue, domain::error::DomainError>)>,
        domain::error::DomainError,
    > {
        Ok(vec![(TagId::new("tag1"), Ok(TagValue::Float(42.0)))])
    }
    async fn write(
        &mut self,
        _id: TagId,
        _val: TagValue,
    ) -> Result<(), domain::error::DomainError> {
        Ok(())
    }
}

struct SequencedDriver {
    state: ConnectionState,
    connect_attempts: Arc<AtomicUsize>,
    fail_connect_until: usize,
    values: Arc<Vec<TagValue>>,
    idx: usize,
}

impl SequencedDriver {
    fn new(
        connect_attempts: Arc<AtomicUsize>,
        fail_connect_until: usize,
        values: Arc<Vec<TagValue>>,
    ) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            connect_attempts,
            fail_connect_until,
            values,
            idx: 0,
        }
    }
}

#[async_trait]
impl DriverConnection for SequencedDriver {
    async fn connect(&mut self) -> Result<(), domain::error::DomainError> {
        let attempt = self.connect_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt <= self.fail_connect_until {
            self.state = ConnectionState::Failed;
            return Err(domain::error::DomainError::DriverError(
                "simulated connect failure".into(),
            ));
        }
        self.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    fn state(&self) -> ConnectionState {
        self.state
    }

    async fn poll(
        &mut self,
    ) -> Result<
        Vec<(TagId, Result<TagValue, domain::error::DomainError>)>,
        domain::error::DomainError,
    > {
        if !self.is_connected() {
            return Err(domain::error::DomainError::DriverError(
                "not connected".into(),
            ));
        }

        let value = self.values[self.idx % self.values.len()].clone();
        self.idx += 1;

        Ok(vec![
            (TagId::new("tag_fast"), Ok(value.clone())),
            (TagId::new("tag_slow"), Ok(value)),
        ])
    }

    async fn write(
        &mut self,
        _id: TagId,
        _val: TagValue,
    ) -> Result<(), domain::error::DomainError> {
        Ok(())
    }
}

struct MockFactory;
impl DriverFactory for MockFactory {
    fn create(&self, _conn: &Connection) -> Box<dyn DriverConnection> {
        Box::new(MockDriver {
            state: ConnectionState::Disconnected,
        })
    }
}

struct SequencedFactory {
    connect_attempts: Arc<AtomicUsize>,
    fail_connect_until: usize,
    values: Arc<Vec<TagValue>>,
}

impl DriverFactory for SequencedFactory {
    fn create(&self, _conn: &Connection) -> Box<dyn DriverConnection> {
        Box::new(SequencedDriver::new(
            self.connect_attempts.clone(),
            self.fail_connect_until,
            self.values.clone(),
        ))
    }
}

enum PollStep {
    Values(Vec<(TagId, TagValue)>),
    Empty,
    Error,
}

struct ScriptedDriver {
    state: ConnectionState,
    steps: Arc<Mutex<VecDeque<PollStep>>>,
}

impl ScriptedDriver {
    fn new(steps: Arc<Mutex<VecDeque<PollStep>>>) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            steps,
        }
    }
}

#[async_trait]
impl DriverConnection for ScriptedDriver {
    async fn connect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    fn state(&self) -> ConnectionState {
        self.state
    }

    async fn poll(
        &mut self,
    ) -> Result<
        Vec<(TagId, Result<TagValue, domain::error::DomainError>)>,
        domain::error::DomainError,
    > {
        let step = self.steps.lock().unwrap().pop_front().unwrap_or(PollStep::Empty);
        match step {
            PollStep::Values(vals) => Ok(vals.into_iter().map(|(id, v)| (id, Ok(v))).collect()),
            PollStep::Empty => Ok(vec![]),
            PollStep::Error => Err(domain::error::DomainError::DriverError(
                "scripted poll error".into(),
            )),
        }
    }

    async fn write(
        &mut self,
        _id: TagId,
        _val: TagValue,
    ) -> Result<(), domain::error::DomainError> {
        Ok(())
    }
}

struct ScriptedFactory {
    steps: Arc<Mutex<VecDeque<PollStep>>>,
}

impl DriverFactory for ScriptedFactory {
    fn create(&self, _conn: &Connection) -> Box<dyn DriverConnection> {
        Box::new(ScriptedDriver::new(self.steps.clone()))
    }
}

struct WriteDriver {
    state: ConnectionState,
    fail_write: bool,
}

impl WriteDriver {
    fn new(fail_write: bool) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            fail_write,
        }
    }
}

#[async_trait]
impl DriverConnection for WriteDriver {
    async fn connect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    fn state(&self) -> ConnectionState {
        self.state
    }

    async fn poll(
        &mut self,
    ) -> Result<
        Vec<(TagId, Result<TagValue, domain::error::DomainError>)>,
        domain::error::DomainError,
    > {
        Ok(vec![])
    }

    async fn write(
        &mut self,
        _id: TagId,
        _val: TagValue,
    ) -> Result<(), domain::error::DomainError> {
        if self.fail_write {
            Err(domain::error::DomainError::DriverError(
                "simulated write failure".into(),
            ))
        } else {
            Ok(())
        }
    }
}

struct WriteFactory {
    fail_write: bool,
}

impl DriverFactory for WriteFactory {
    fn create(&self, _conn: &Connection) -> Box<dyn DriverConnection> {
        Box::new(WriteDriver::new(self.fail_write))
    }
}

struct CountingWriteDriver {
    state: ConnectionState,
    counts: Arc<Mutex<HashMap<String, usize>>>,
}

impl CountingWriteDriver {
    fn new(counts: Arc<Mutex<HashMap<String, usize>>>) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            counts,
        }
    }
}

#[async_trait]
impl DriverConnection for CountingWriteDriver {
    async fn connect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    fn state(&self) -> ConnectionState {
        self.state
    }

    async fn poll(
        &mut self,
    ) -> Result<
        Vec<(TagId, Result<TagValue, domain::error::DomainError>)>,
        domain::error::DomainError,
    > {
        Ok(vec![])
    }

    async fn write(
        &mut self,
        id: TagId,
        _val: TagValue,
    ) -> Result<(), domain::error::DomainError> {
        let mut map = self.counts.lock().unwrap();
        let entry = map.entry(id.to_string()).or_insert(0);
        *entry += 1;
        Ok(())
    }
}

struct CountingWriteFactory {
    counts: Arc<Mutex<HashMap<String, usize>>>,
}

impl DriverFactory for CountingWriteFactory {
    fn create(&self, _conn: &Connection) -> Box<dyn DriverConnection> {
        Box::new(CountingWriteDriver::new(self.counts.clone()))
    }
}

struct OrderedWriteDriver {
    state: ConnectionState,
    order: Arc<Mutex<Vec<String>>>,
}

impl OrderedWriteDriver {
    fn new(order: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            order,
        }
    }
}

#[async_trait]
impl DriverConnection for OrderedWriteDriver {
    async fn connect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Connected;
        Ok(())
    }
    async fn disconnect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Disconnected;
        Ok(())
    }
    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }
    fn state(&self) -> ConnectionState {
        self.state
    }
    async fn poll(
        &mut self,
    ) -> Result<
        Vec<(TagId, Result<TagValue, domain::error::DomainError>)>,
        domain::error::DomainError,
    > {
        Ok(vec![])
    }
    async fn write(
        &mut self,
        id: TagId,
        _val: TagValue,
    ) -> Result<(), domain::error::DomainError> {
        self.order.lock().unwrap().push(id.to_string());
        Ok(())
    }
}

struct OrderedWriteFactory {
    order: Arc<Mutex<Vec<String>>>,
}

impl DriverFactory for OrderedWriteFactory {
    fn create(&self, _conn: &Connection) -> Box<dyn DriverConnection> {
        Box::new(OrderedWriteDriver::new(self.order.clone()))
    }
}

struct DelayedOrderedWriteDriver {
    state: ConnectionState,
    order: Arc<Mutex<Vec<String>>>,
    delay_ms: u64,
}

impl DelayedOrderedWriteDriver {
    fn new(order: Arc<Mutex<Vec<String>>>, delay_ms: u64) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            order,
            delay_ms,
        }
    }
}

#[async_trait]
impl DriverConnection for DelayedOrderedWriteDriver {
    async fn connect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Connected;
        Ok(())
    }
    async fn disconnect(&mut self) -> Result<(), domain::error::DomainError> {
        self.state = ConnectionState::Disconnected;
        Ok(())
    }
    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }
    fn state(&self) -> ConnectionState {
        self.state
    }
    async fn poll(
        &mut self,
    ) -> Result<
        Vec<(TagId, Result<TagValue, domain::error::DomainError>)>,
        domain::error::DomainError,
    > {
        Ok(vec![])
    }
    async fn write(
        &mut self,
        id: TagId,
        _val: TagValue,
    ) -> Result<(), domain::error::DomainError> {
        sleep(Duration::from_millis(self.delay_ms)).await;
        self.order.lock().unwrap().push(id.to_string());
        Ok(())
    }
}

struct DelayedOrderedWriteFactory {
    order: Arc<Mutex<Vec<String>>>,
    delay_ms: u64,
}

impl DriverFactory for DelayedOrderedWriteFactory {
    fn create(&self, _conn: &Connection) -> Box<dyn DriverConnection> {
        Box::new(DelayedOrderedWriteDriver::new(
            self.order.clone(),
            self.delay_ms,
        ))
    }
}

#[tokio::test]
async fn test_runtime_engine_flow() {
    let mut registry = DriverRegistry::new();
    registry.register(DriverType::new("mock").unwrap(), Box::new(MockFactory));

    let mut engine = RuntimeEngine::new(registry);
    let live_state = engine.live_state();

    let conn = Connection::new(
        ConnectionId::new("conn1"),
        "Test Conn".into(),
        DriverType::new("mock").unwrap(),
        serde_json::json!({}),
    );

    let tag = Tag::new(
        TagId::new("tag1"),
        "Test Tag".into(),
        DeviceId::new("dev1"),
        "address:1".into(),
    );

    engine.start_connection(conn, vec![tag]).await.unwrap();

    // Wait for at least one poll
    sleep(Duration::from_millis(500)).await;

    let state = live_state
        .get_tag(&TagId::new("tag1"))
        .expect("Tag should be in live state");
    assert_eq!(state.value, TagValue::Float(42.0));
    assert_eq!(state.quality.status, domain::tag::QualityStatus::Good);

    engine.stop_all().await;
}

#[tokio::test]
async fn test_in_memory_repository() {
    use domain::tag::TagRepository;
    use infrastructure::repositories::in_memory::InMemoryTagRepository;

    let repo = InMemoryTagRepository::new();
    let tag_id = TagId::new("repo-tag");
    let tag = Tag::new(
        tag_id.clone(),
        "Repo Tag".into(),
        DeviceId::new("dev1"),
        "addr:1".into(),
    );

    repo.save(tag).await.unwrap();
    let found = repo
        .find_by_id(&tag_id)
        .await
        .unwrap()
        .expect("Tag should be found");
    assert_eq!(found.name, "Repo Tag");

    let all = repo.find_all().await.unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn test_runtime_respects_tag_update_mode_intervals() {
    let connect_attempts = Arc::new(AtomicUsize::new(0));
    let values = Arc::new(vec![
        TagValue::Float(1.0),
        TagValue::Float(2.0),
        TagValue::Float(3.0),
        TagValue::Float(4.0),
    ]);

    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("seq").unwrap(),
        Box::new(SequencedFactory {
            connect_attempts,
            fail_connect_until: 0,
            values,
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut events = engine.event_bus().subscribe();

    let mut conn = Connection::new(
        ConnectionId::new("conn_sched"),
        "Conn Sched".into(),
        DriverType::new("seq").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 30;

    let mut tag_fast = Tag::new(
        TagId::new("tag_fast"),
        "Fast".into(),
        DeviceId::new("dev1"),
        "addr:fast".into(),
    );
    tag_fast.update_mode = TagUpdateMode::Polling { interval_ms: 30 };

    let mut tag_slow = Tag::new(
        TagId::new("tag_slow"),
        "Slow".into(),
        DeviceId::new("dev1"),
        "addr:slow".into(),
    );
    tag_slow.update_mode = TagUpdateMode::Polling { interval_ms: 120 };

    engine
        .start_connection(conn, vec![tag_fast, tag_slow])
        .await
        .unwrap();

    sleep(Duration::from_millis(320)).await;
    engine.stop_all().await;

    let mut fast_count = 0usize;
    let mut slow_count = 0usize;
    while let Ok(evt) = events.try_recv() {
        if let application::runtime::RuntimeEvent::TagChanged { tag_id, .. } = evt {
            if tag_id == TagId::new("tag_fast") {
                fast_count += 1;
            } else if tag_id == TagId::new("tag_slow") {
                slow_count += 1;
            }
        }
    }

    assert!(fast_count >= 4, "expected fast tag to emit at least 4 times");
    assert!(slow_count >= 1, "expected slow tag to emit at least once");
    assert!(
        fast_count > slow_count,
        "expected fast tag events ({fast_count}) > slow tag events ({slow_count})"
    );
}

#[tokio::test]
async fn test_runtime_on_message_publishes_repeated_equal_values() {
    let steps = Arc::new(Mutex::new(VecDeque::from(vec![
        PollStep::Values(vec![(TagId::new("tag_scale"), TagValue::Float(0.1))]),
        PollStep::Values(vec![(TagId::new("tag_scale"), TagValue::Float(0.1))]),
    ])));

    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("scripted_on_message").unwrap(),
        Box::new(ScriptedFactory {
            steps: steps.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut events = engine.event_bus().subscribe();

    let mut conn = Connection::new(
        ConnectionId::new("conn_on_message"),
        "Conn OnMessage".into(),
        DriverType::new("scripted_on_message").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let mut tag = Tag::new(
        TagId::new("tag_scale"),
        "Scale".into(),
        DeviceId::new("dev_scale"),
        "scale:compound".into(),
    );
    tag.update_mode = TagUpdateMode::OnMessage;

    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(120)).await;
    engine.stop_all().await;

    let mut count = 0usize;
    while let Ok(evt) = events.try_recv() {
        if let application::runtime::RuntimeEvent::TagChanged { tag_id, .. } = evt {
            if tag_id == TagId::new("tag_scale") {
                count += 1;
            }
        }
    }

    assert!(
        count >= 2,
        "expected on_message to publish repeated values; got {count}"
    );
}

#[tokio::test]
async fn test_runtime_honors_reconnection_max_retries() {
    let connect_attempts = Arc::new(AtomicUsize::new(0));
    let values = Arc::new(vec![TagValue::Float(1.0)]);

    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("seq_fail").unwrap(),
        Box::new(SequencedFactory {
            connect_attempts: connect_attempts.clone(),
            fail_connect_until: usize::MAX, // always fail
            values,
        }),
    );

    let mut engine = RuntimeEngine::new(registry);

    let mut conn = Connection::new(
        ConnectionId::new("conn_retry"),
        "Conn Retry".into(),
        DriverType::new("seq_fail").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;
    conn.reconnection = ReconnectionPolicy {
        strategy: ReconnectStrategy::Fixed { delay_ms: 40 },
        max_retries: Some(3),
    };

    let tag = Tag::new(
        TagId::new("tag_fast"),
        "Fast".into(),
        DeviceId::new("dev1"),
        "addr:fast".into(),
    );

    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(320)).await;
    engine.stop_all().await;

    let attempts = connect_attempts.load(Ordering::SeqCst);
    assert_eq!(
        attempts, 3,
        "expected connect attempts to stop at max_retries (3), got {attempts}"
    );
}

#[tokio::test]
async fn test_runtime_exponential_reconnect_backoff() {
    let connect_attempts = Arc::new(AtomicUsize::new(0));
    let values = Arc::new(vec![TagValue::Float(1.0)]);

    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("seq_exp").unwrap(),
        Box::new(SequencedFactory {
            connect_attempts: connect_attempts.clone(),
            fail_connect_until: usize::MAX, // always fail
            values,
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_exp"),
        "Conn Exp".into(),
        DriverType::new("seq_exp").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 10;
    conn.reconnection = ReconnectionPolicy {
        strategy: ReconnectStrategy::Exponential {
            initial_delay_ms: 20,
            max_delay_ms: 80,
        },
        max_retries: Some(4),
    };

    let tag = Tag::new(
        TagId::new("tag_fast"),
        "Fast".into(),
        DeviceId::new("dev1"),
        "addr:fast".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();

    // With runtime loop ticking at >=100ms, retries are evaluated on each tick.
    sleep(Duration::from_millis(130)).await;
    let attempts_130 = connect_attempts.load(Ordering::SeqCst);
    assert_eq!(attempts_130, 2, "expected first retry by ~100ms");

    sleep(Duration::from_millis(120)).await;
    let attempts_250 = connect_attempts.load(Ordering::SeqCst);
    assert!(
        attempts_250 >= 2 && attempts_250 <= 3,
        "expected 2 or 3 attempts by 250ms, got {attempts_250}"
    );

    // By this point the 4th retry window should have been consumed and capped by max_retries.
    sleep(Duration::from_millis(140)).await;
    let attempts_390 = connect_attempts.load(Ordering::SeqCst);
    assert_eq!(
        attempts_390, 4,
        "expected retries to stop at max_retries=4, got {attempts_390}"
    );

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_reload_resets_retry_budget() {
    let connect_attempts = Arc::new(AtomicUsize::new(0));
    let values = Arc::new(vec![TagValue::Float(1.0)]);

    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("seq_reload").unwrap(),
        Box::new(SequencedFactory {
            connect_attempts: connect_attempts.clone(),
            fail_connect_until: usize::MAX, // always fail
            values,
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let conn_id = ConnectionId::new("conn_reload");

    let mut conn = Connection::new(
        conn_id.clone(),
        "Conn Reload".into(),
        DriverType::new("seq_reload").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 10;
    conn.reconnection = ReconnectionPolicy {
        strategy: ReconnectStrategy::Fixed { delay_ms: 10 },
        max_retries: Some(1),
    };

    let tag = Tag::new(
        TagId::new("tag_fast"),
        "Fast".into(),
        DeviceId::new("dev1"),
        "addr:fast".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();

    sleep(Duration::from_millis(80)).await;
    assert_eq!(
        connect_attempts.load(Ordering::SeqCst),
        1,
        "expected first config to exhaust at max_retries=1"
    );

    let mut reloaded = Connection::new(
        conn_id.clone(),
        "Conn Reload".into(),
        DriverType::new("seq_reload").unwrap(),
        serde_json::json!({}),
    );
    reloaded.timeout_ms = 10;
    reloaded.reconnection = ReconnectionPolicy {
        strategy: ReconnectStrategy::Fixed { delay_ms: 10 },
        max_retries: Some(3),
    };

    engine.reload_connection(reloaded).await.unwrap();

    // After reload it should get a fresh retry budget and attempt 3 additional connects.
    sleep(Duration::from_millis(120)).await;
    assert!(
        connect_attempts.load(Ordering::SeqCst) >= 2,
        "expected at least one additional retry after reload"
    );

    sleep(Duration::from_millis(260)).await;
    assert_eq!(
        connect_attempts.load(Ordering::SeqCst),
        4,
        "expected 1 original + 3 retries after reload"
    );

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_marks_tag_timeout_quality_when_data_stops() {
    let steps = Arc::new(Mutex::new(VecDeque::from([
        PollStep::Values(vec![(TagId::new("tag_timeout"), TagValue::Float(10.0))]),
        PollStep::Empty,
        PollStep::Empty,
        PollStep::Empty,
        PollStep::Empty,
    ])));

    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("script_timeout").unwrap(),
        Box::new(ScriptedFactory {
            steps: steps.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let live_state = engine.live_state();

    let mut conn = Connection::new(
        ConnectionId::new("conn_timeout"),
        "Conn Timeout".into(),
        DriverType::new("script_timeout").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 50;

    let mut tag = Tag::new(
        TagId::new("tag_timeout"),
        "Tag Timeout".into(),
        DeviceId::new("dev_timeout"),
        "addr:1".into(),
    );
    tag.update_mode = TagUpdateMode::Polling { interval_ms: 30 };

    engine.start_connection(conn, vec![tag]).await.unwrap();

    sleep(Duration::from_millis(420)).await;

    let state = live_state
        .get_tag(&TagId::new("tag_timeout"))
        .expect("tag_timeout must exist in live state");

    assert_eq!(state.quality.status, domain::tag::QualityStatus::Bad);
    assert_eq!(
        state.quality.reason,
        Some(domain::tag::QualityReason::Timeout)
    );

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_marks_communication_failure_and_recovers_to_good() {
    let steps = Arc::new(Mutex::new(VecDeque::from([
        PollStep::Values(vec![(TagId::new("tag_comm"), TagValue::Float(10.0))]),
        PollStep::Error,
        PollStep::Values(vec![(TagId::new("tag_comm"), TagValue::Float(11.0))]),
        PollStep::Empty,
    ])));

    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("script_comm").unwrap(),
        Box::new(ScriptedFactory {
            steps: steps.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let live_state = engine.live_state();
    let mut events = engine.event_bus().subscribe();

    let mut conn = Connection::new(
        ConnectionId::new("conn_comm"),
        "Conn Comm".into(),
        DriverType::new("script_comm").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 50;
    conn.reconnection = ReconnectionPolicy {
        strategy: ReconnectStrategy::Fixed { delay_ms: 20 },
        max_retries: Some(5),
    };

    let mut tag = Tag::new(
        TagId::new("tag_comm"),
        "Tag Comm".into(),
        DeviceId::new("dev_comm"),
        "addr:2".into(),
    );
    tag.update_mode = TagUpdateMode::Polling { interval_ms: 30 };

    engine.start_connection(conn, vec![tag]).await.unwrap();

    sleep(Duration::from_millis(520)).await;

    let final_state = live_state
        .get_tag(&TagId::new("tag_comm"))
        .expect("tag_comm must exist in live state");
    assert_eq!(final_state.value, TagValue::Float(11.0));

    let mut saw_comm_bad = false;
    let mut saw_recovery_good = false;
    while let Ok(evt) = events.try_recv() {
        if let application::runtime::RuntimeEvent::TagChanged {
            tag_id,
            quality,
            value,
            ..
        } = evt
        {
            if tag_id == TagId::new("tag_comm")
                && quality.status == domain::tag::QualityStatus::Bad
                && quality.reason == Some(domain::tag::QualityReason::CommunicationFailure)
            {
                saw_comm_bad = true;
            }
            if tag_id == TagId::new("tag_comm")
                && quality.status == domain::tag::QualityStatus::Good
                && value == TagValue::Float(11.0)
            {
                saw_recovery_good = true;
            }
        }
    }

    assert!(saw_comm_bad, "expected a bad communication quality event");
    assert!(
        saw_recovery_good,
        "expected recovery to good quality on first valid value after communication failure"
    );

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_tag_success_updates_live_state_and_events() {
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_ok").unwrap(),
        Box::new(WriteFactory { fail_write: false }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let live_state = engine.live_state();
    let mut events = engine.event_bus().subscribe();

    let mut conn = Connection::new(
        ConnectionId::new("conn_write_ok"),
        "Conn Write OK".into(),
        DriverType::new("write_ok").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_cmd_ok");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Cmd OK".into(),
        DeviceId::new("dev_cmd_ok"),
        "addr:w".into(),
    );

    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(80)).await;

    engine
        .write_tag(tag_id.clone(), TagValue::Float(77.7))
        .await
        .unwrap();

    sleep(Duration::from_millis(40)).await;
    let state = live_state.get_tag(&tag_id).expect("tag state expected");
    assert_eq!(state.value, TagValue::Float(77.7));
    assert_eq!(state.quality.status, domain::tag::QualityStatus::Good);

    let mut saw_event = false;
    while let Ok(evt) = events.try_recv() {
        if let application::runtime::RuntimeEvent::TagChanged { tag_id: id, value, .. } = evt {
            if id == tag_id && value == TagValue::Float(77.7) {
                saw_event = true;
                break;
            }
        }
    }
    assert!(saw_event, "expected tag write event");

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_unknown_tag_returns_not_found() {
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_unknown").unwrap(),
        Box::new(WriteFactory { fail_write: false }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_unknown"),
        "Conn Write Unknown".into(),
        DriverType::new("write_unknown").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let tag = Tag::new(
        TagId::new("tag_present"),
        "Tag Present".into(),
        DeviceId::new("dev_present"),
        "addr:p".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    let err = engine
        .write_tag(TagId::new("tag_absent"), TagValue::Float(1.0))
        .await
        .expect_err("unknown tag should fail");

    match err {
        domain::error::DomainError::NotFound(_) => {}
        other => panic!("expected NotFound, got {:?}", other),
    }

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_error_marks_bad_communication() {
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_fail").unwrap(),
        Box::new(WriteFactory { fail_write: true }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let live_state = engine.live_state();

    let mut conn = Connection::new(
        ConnectionId::new("conn_write_fail"),
        "Conn Write Fail".into(),
        DriverType::new("write_fail").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_cmd_fail");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Cmd Fail".into(),
        DeviceId::new("dev_cmd_fail"),
        "addr:wf".into(),
    );

    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(80)).await;

    let err = engine
        .write_tag(tag_id.clone(), TagValue::Float(12.3))
        .await
        .expect_err("write should fail");
    match err {
        domain::error::DomainError::DriverError(_) => {}
        other => panic!("expected DriverError, got {:?}", other),
    }

    sleep(Duration::from_millis(40)).await;
    let state = live_state.get_tag(&tag_id).expect("tag state expected");
    assert_eq!(state.quality.status, domain::tag::QualityStatus::Bad);
    assert_eq!(
        state.quality.reason,
        Some(domain::tag::QualityReason::CommunicationFailure)
    );

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_command_id_deduplicates_same_tag() {
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_count").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_dedup"),
        "Conn Dedup".into(),
        DriverType::new("write_count").unwrap(),
        serde_json::json!({"command_dedup_ms": 1000}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_dedup");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Dedup".into(),
        DeviceId::new("dev_dedup"),
        "addr:d".into(),
    );

    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(1.0),
            "cmd-001".to_string(),
        )
        .await
        .unwrap();
    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(1.0),
            "cmd-001".to_string(),
        )
        .await
        .unwrap();

    sleep(Duration::from_millis(20)).await;
    let map = counts.lock().unwrap();
    assert_eq!(map.get(&tag_id.to_string()).copied().unwrap_or(0), 1);

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_command_id_is_scoped_per_tag() {
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_count_scope").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_scope"),
        "Conn Scope".into(),
        DriverType::new("write_count_scope").unwrap(),
        serde_json::json!({"command_dedup_ms": 1000}),
    );
    conn.timeout_ms = 20;

    let tag_a = TagId::new("tag_scope_a");
    let tag_b = TagId::new("tag_scope_b");
    let t1 = Tag::new(
        tag_a.clone(),
        "Tag Scope A".into(),
        DeviceId::new("dev_scope"),
        "addr:a".into(),
    );
    let t2 = Tag::new(
        tag_b.clone(),
        "Tag Scope B".into(),
        DeviceId::new("dev_scope"),
        "addr:b".into(),
    );
    engine.start_connection(conn, vec![t1, t2]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    engine
        .write_tag_with_command_id(tag_a.clone(), TagValue::Float(1.0), "same-cmd".to_string())
        .await
        .unwrap();
    engine
        .write_tag_with_command_id(tag_b.clone(), TagValue::Float(2.0), "same-cmd".to_string())
        .await
        .unwrap();

    sleep(Duration::from_millis(20)).await;
    let map = counts.lock().unwrap();
    assert_eq!(map.get(&tag_a.to_string()).copied().unwrap_or(0), 1);
    assert_eq!(map.get(&tag_b.to_string()).copied().unwrap_or(0), 1);

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_command_id_expires_after_window() {
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_count_expire").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_expire"),
        "Conn Expire".into(),
        DriverType::new("write_count_expire").unwrap(),
        serde_json::json!({"command_dedup_ms": 120}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_expire");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Expire".into(),
        DeviceId::new("dev_expire"),
        "addr:e".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(1.0),
            "cmd-exp".to_string(),
        )
        .await
        .unwrap();

    // Duplicate inside window -> deduped
    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(1.0),
            "cmd-exp".to_string(),
        )
        .await
        .unwrap();

    sleep(Duration::from_millis(30)).await;
    {
        let map = counts.lock().unwrap();
        assert_eq!(map.get(&tag_id.to_string()).copied().unwrap_or(0), 1);
    }

    // After window expires -> should execute again
    sleep(Duration::from_millis(140)).await;
    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(2.0),
            "cmd-exp".to_string(),
        )
        .await
        .unwrap();

    sleep(Duration::from_millis(20)).await;
    let map = counts.lock().unwrap();
    assert_eq!(map.get(&tag_id.to_string()).copied().unwrap_or(0), 2);

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_rate_limit_rejects_burst_same_tag() {
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_rate_limit").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_rate_limit"),
        "Conn Write Rate Limit".into(),
        DriverType::new("write_rate_limit").unwrap(),
        serde_json::json!({"write_rate_limit_ms": 200}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_write_rate_limit");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Write Rate Limit".into(),
        DeviceId::new("dev_write_rate_limit"),
        "addr:wrl".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    engine
        .write_tag(tag_id.clone(), TagValue::Float(1.0))
        .await
        .unwrap();

    let err = engine
        .write_tag(tag_id.clone(), TagValue::Float(2.0))
        .await
        .expect_err("second write should be rate limited");
    assert!(err.to_string().contains("write rate limited"));

    sleep(Duration::from_millis(220)).await;
    engine
        .write_tag(tag_id.clone(), TagValue::Float(3.0))
        .await
        .unwrap();

    sleep(Duration::from_millis(20)).await;
    let map = counts.lock().unwrap();
    assert_eq!(map.get(&tag_id.to_string()).copied().unwrap_or(0), 2);
    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_rate_limit_is_scoped_per_tag() {
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_rate_limit_scope").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_rate_limit_scope"),
        "Conn Write Rate Limit Scope".into(),
        DriverType::new("write_rate_limit_scope").unwrap(),
        serde_json::json!({"write_rate_limit_ms": 500}),
    );
    conn.timeout_ms = 20;

    let tag_a = TagId::new("tag_write_rate_a");
    let tag_b = TagId::new("tag_write_rate_b");
    let t1 = Tag::new(
        tag_a.clone(),
        "Tag A".into(),
        DeviceId::new("dev_write_rate_scope"),
        "addr:wra".into(),
    );
    let t2 = Tag::new(
        tag_b.clone(),
        "Tag B".into(),
        DeviceId::new("dev_write_rate_scope"),
        "addr:wrb".into(),
    );
    engine.start_connection(conn, vec![t1, t2]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    engine
        .write_tag(tag_a.clone(), TagValue::Float(10.0))
        .await
        .unwrap();
    engine
        .write_tag(tag_b.clone(), TagValue::Float(20.0))
        .await
        .unwrap();

    sleep(Duration::from_millis(20)).await;
    let map = counts.lock().unwrap();
    assert_eq!(map.get(&tag_a.to_string()).copied().unwrap_or(0), 1);
    assert_eq!(map.get(&tag_b.to_string()).copied().unwrap_or(0), 1);
    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_circuit_breaker_opens_after_consecutive_failures() {
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_cb_fail").unwrap(),
        Box::new(WriteFactory { fail_write: true }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_cb_fail"),
        "Conn Write CB Fail".into(),
        DriverType::new("write_cb_fail").unwrap(),
        serde_json::json!({
            "write_circuit_fail_threshold": 2,
            "write_circuit_cooldown_ms": 300
        }),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_write_cb");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Write CB".into(),
        DeviceId::new("dev_write_cb"),
        "addr:wcb".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    let _ = engine.write_tag(tag_id.clone(), TagValue::Float(1.0)).await;
    let _ = engine.write_tag(tag_id.clone(), TagValue::Float(2.0)).await;
    let err = engine
        .write_tag(tag_id.clone(), TagValue::Float(3.0))
        .await
        .expect_err("circuit should be open after threshold failures");
    assert!(err.to_string().contains("circuit breaker open"));

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_circuit_breaker_recovers_after_cooldown() {
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_cb_recover").unwrap(),
        Box::new(WriteFactory { fail_write: false }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_cb_recover"),
        "Conn Write CB Recover".into(),
        DriverType::new("write_cb_recover").unwrap(),
        serde_json::json!({
            "write_circuit_fail_threshold": 1,
            "write_circuit_cooldown_ms": 120,
            "write_rate_limit_ms": 0
        }),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_write_cb_recover");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Write CB Recover".into(),
        DeviceId::new("dev_write_cb_recover"),
        "addr:wcbr".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    // Simulate an open circuit by forcing one failure through a failing runtime.
    // We stop and recreate runtime with failing driver, then restore healthy driver.
    engine.stop_all().await;

    let mut registry_fail = DriverRegistry::new();
    registry_fail.register(
        DriverType::new("write_cb_recover").unwrap(),
        Box::new(WriteFactory { fail_write: true }),
    );
    let mut engine_fail = RuntimeEngine::new(registry_fail);
    let mut conn_fail = Connection::new(
        ConnectionId::new("conn_write_cb_recover"),
        "Conn Write CB Recover".into(),
        DriverType::new("write_cb_recover").unwrap(),
        serde_json::json!({
            "write_circuit_fail_threshold": 1,
            "write_circuit_cooldown_ms": 120
        }),
    );
    conn_fail.timeout_ms = 20;
    let tag_fail = Tag::new(
        tag_id.clone(),
        "Tag Write CB Recover".into(),
        DeviceId::new("dev_write_cb_recover"),
        "addr:wcbr".into(),
    );
    engine_fail.start_connection(conn_fail, vec![tag_fail]).await.unwrap();
    sleep(Duration::from_millis(50)).await;
    let _ = engine_fail.write_tag(tag_id.clone(), TagValue::Float(1.0)).await;
    let err_open = engine_fail
        .write_tag(tag_id.clone(), TagValue::Float(2.0))
        .await
        .expect_err("circuit should be open");
    assert!(err_open.to_string().contains("circuit breaker open"));
    sleep(Duration::from_millis(140)).await;
    let err_driver = engine_fail
        .write_tag(tag_id.clone(), TagValue::Float(3.0))
        .await
        .expect_err("after cooldown, failure should be driver-level again");
    assert!(err_driver.to_string().contains("simulated write failure"));
    engine_fail.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_priority_processes_high_before_normal() {
    let order = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_priority").unwrap(),
        Box::new(OrderedWriteFactory {
            order: order.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_priority"),
        "Conn Write Priority".into(),
        DriverType::new("write_priority").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let tag_low = TagId::new("tag_low_priority");
    let tag_high = TagId::new("tag_high_priority");
    let t1 = Tag::new(
        tag_low.clone(),
        "Tag Low".into(),
        DeviceId::new("dev_write_priority"),
        "addr:wpl".into(),
    );
    let t2 = Tag::new(
        tag_high.clone(),
        "Tag High".into(),
        DeviceId::new("dev_write_priority"),
        "addr:wph".into(),
    );
    engine.start_connection(conn, vec![t1, t2]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    let low_fut =
        engine.write_tag_with_priority(tag_low.clone(), TagValue::Float(1.0), WritePriority::Normal);
    let high_fut =
        engine.write_tag_with_priority(tag_high.clone(), TagValue::Float(2.0), WritePriority::High);
    let (_low, _high) = tokio::join!(low_fut, high_fut);

    sleep(Duration::from_millis(120)).await;
    let written = order.lock().unwrap().clone();
    assert_eq!(written.len(), 2);
    assert_eq!(written[0], tag_high.to_string());
    assert_eq!(written[1], tag_low.to_string());
    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_priority_burst_drains_high_queue_before_normal_queue() {
    let order = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_priority_burst").unwrap(),
        Box::new(DelayedOrderedWriteFactory {
            order: order.clone(),
            delay_ms: 20,
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_priority_burst"),
        "Conn Write Priority Burst".into(),
        DriverType::new("write_priority_burst").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let tag_normal = TagId::new("tag_normal_priority");
    let tag_high = TagId::new("tag_high_priority_burst");
    let t1 = Tag::new(
        tag_normal.clone(),
        "Tag Normal".into(),
        DeviceId::new("dev_write_priority_burst"),
        "addr:wpbn".into(),
    );
    let t2 = Tag::new(
        tag_high.clone(),
        "Tag High".into(),
        DeviceId::new("dev_write_priority_burst"),
        "addr:wpbh".into(),
    );
    engine.start_connection(conn, vec![t1, t2]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    let engine = Arc::new(engine);
    let prime_engine = engine.clone();
    let prime_normal = tag_normal.clone();
    let prime = tokio::spawn(async move {
        prime_engine
            .write_tag_with_priority(prime_normal, TagValue::Float(1.0), WritePriority::Normal)
            .await
            .unwrap();
    });

    // Let first normal write start so the rest accumulates in pending queues.
    sleep(Duration::from_millis(5)).await;

    let mut handles = Vec::new();
    for i in 0..8 {
        let e = engine.clone();
        let t = tag_normal.clone();
        handles.push(tokio::spawn(async move {
            e.write_tag_with_priority(t, TagValue::Float(10.0 + i as f64), WritePriority::Normal)
                .await
                .unwrap();
        }));
    }
    for i in 0..4 {
        let e = engine.clone();
        let t = tag_high.clone();
        handles.push(tokio::spawn(async move {
            e.write_tag_with_priority(t, TagValue::Float(100.0 + i as f64), WritePriority::High)
                .await
                .unwrap();
        }));
    }

    prime.await.unwrap();
    for h in handles {
        h.await.unwrap();
    }

    sleep(Duration::from_millis(350)).await;
    let written = order.lock().unwrap().clone();
    assert_eq!(written.len(), 13);
    let high_count = written
        .iter()
        .filter(|id| **id == tag_high.to_string())
        .count();
    let normal_count = written
        .iter()
        .filter(|id| **id == tag_normal.to_string())
        .count();
    assert_eq!(high_count, 4, "expected 4 high-priority writes");
    assert_eq!(normal_count, 9, "expected 9 normal-priority writes");

    // Queue guarantee: after first high-priority command is observed,
    // high writes must not appear again after a later normal write.
    let first_high = written
        .iter()
        .position(|id| id == &tag_high.to_string())
        .expect("expected at least one high-priority write");
    let mut saw_normal_after_high = false;
    for id in written.iter().skip(first_high) {
        if id == &tag_normal.to_string() {
            saw_normal_after_high = true;
            continue;
        }
        if id == &tag_high.to_string() {
            assert!(
                !saw_normal_after_high,
                "high-priority write appeared after normal writes: {:?}",
                written
            );
        }
    }

    let mut engine = match Arc::try_unwrap(engine) {
        Ok(e) => e,
        Err(_) => panic!("engine still shared at test teardown"),
    };
    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_audit_emits_applied_and_deduplicated_outcomes() {
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_audit_count").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut events = engine.event_bus().subscribe();
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_audit"),
        "Conn Write Audit".into(),
        DriverType::new("write_audit_count").unwrap(),
        serde_json::json!({"command_dedup_ms": 1000}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_write_audit");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Write Audit".into(),
        DeviceId::new("dev_write_audit"),
        "addr:wa".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(1.0),
            "audit-cmd-1".to_string(),
        )
        .await
        .unwrap();
    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(1.0),
            "audit-cmd-1".to_string(),
        )
        .await
        .unwrap();

    sleep(Duration::from_millis(20)).await;

    let mut saw_applied = false;
    let mut saw_dedup = false;
    while let Ok(evt) = events.try_recv() {
        if let application::runtime::RuntimeEvent::TagWriteCommandHandled {
            tag_id: id,
            command_id,
            outcome,
            ..
        } = evt
        {
            if id == tag_id && command_id.as_deref() == Some("audit-cmd-1") {
                if outcome == application::runtime::WriteCommandOutcome::Applied {
                    saw_applied = true;
                }
                if outcome == application::runtime::WriteCommandOutcome::Deduplicated {
                    saw_dedup = true;
                }
            }
        }
    }

    assert!(saw_applied, "expected applied write audit event");
    assert!(saw_dedup, "expected deduplicated write audit event");
    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_audit_emits_rejected_on_driver_error() {
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_audit_fail").unwrap(),
        Box::new(WriteFactory { fail_write: true }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut events = engine.event_bus().subscribe();

    let mut conn = Connection::new(
        ConnectionId::new("conn_write_audit_fail"),
        "Conn Write Audit Fail".into(),
        DriverType::new("write_audit_fail").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_write_audit_fail");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Write Audit Fail".into(),
        DeviceId::new("dev_write_audit_fail"),
        "addr:waf".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    let _ = engine
        .write_tag(tag_id.clone(), TagValue::Float(9.9))
        .await
        .expect_err("write should fail");

    sleep(Duration::from_millis(20)).await;

    let mut saw_rejected = false;
    while let Ok(evt) = events.try_recv() {
        if let application::runtime::RuntimeEvent::TagWriteCommandHandled {
            tag_id: id,
            outcome,
            reason,
            ..
        } = evt
        {
            if id == tag_id
                && outcome == application::runtime::WriteCommandOutcome::Rejected
                && reason.unwrap_or_default().contains("simulated write failure")
            {
                saw_rejected = true;
                break;
            }
        }
    }

    assert!(saw_rejected, "expected rejected write audit event");
    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_audit_emits_rejected_for_unknown_tag_route() {
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_audit_unknown").unwrap(),
        Box::new(WriteFactory { fail_write: false }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut events = engine.event_bus().subscribe();
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_audit_unknown"),
        "Conn Write Audit Unknown".into(),
        DriverType::new("write_audit_unknown").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let known_tag = Tag::new(
        TagId::new("tag_known"),
        "Tag Known".into(),
        DeviceId::new("dev_known"),
        "addr:known".into(),
    );
    engine.start_connection(conn, vec![known_tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    let unknown_tag = TagId::new("tag_unknown_route");
    let _ = engine
        .write_tag(unknown_tag.clone(), TagValue::Float(3.3))
        .await
        .expect_err("unknown tag route should fail");

    sleep(Duration::from_millis(20)).await;

    let mut saw_rejected = false;
    while let Ok(evt) = events.try_recv() {
        if let application::runtime::RuntimeEvent::TagWriteCommandHandled {
            connection_id,
            tag_id: id,
            outcome,
            reason,
            ..
        } = evt
        {
            if id == unknown_tag
                && connection_id.is_none()
                && outcome == application::runtime::WriteCommandOutcome::Rejected
                && reason.unwrap_or_default().contains("tag route not found")
            {
                saw_rejected = true;
                break;
            }
        }
    }

    assert!(saw_rejected, "expected rejected audit event for unknown tag route");
    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_audit_store_queries_by_tag_and_command_id() {
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_audit_store").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_audit_store"),
        "Conn Write Audit Store".into(),
        DriverType::new("write_audit_store").unwrap(),
        serde_json::json!({"command_dedup_ms": 1000}),
    );
    conn.timeout_ms = 20;

    let tag_a = TagId::new("tag_audit_store_a");
    let tag_b = TagId::new("tag_audit_store_b");
    let t1 = Tag::new(
        tag_a.clone(),
        "Tag A".into(),
        DeviceId::new("dev_audit_store"),
        "addr:a".into(),
    );
    let t2 = Tag::new(
        tag_b.clone(),
        "Tag B".into(),
        DeviceId::new("dev_audit_store"),
        "addr:b".into(),
    );
    engine.start_connection(conn, vec![t1, t2]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    engine
        .write_tag_with_command_id(tag_a.clone(), TagValue::Float(1.0), "cmd-x".to_string())
        .await
        .unwrap();
    engine
        .write_tag_with_command_id(tag_a.clone(), TagValue::Float(1.0), "cmd-x".to_string())
        .await
        .unwrap();
    engine
        .write_tag_with_command_id(tag_b.clone(), TagValue::Float(2.0), "cmd-y".to_string())
        .await
        .unwrap();
    sleep(Duration::from_millis(30)).await;

    let by_tag_a = engine.write_audit_by_tag(&tag_a).unwrap();
    assert_eq!(by_tag_a.len(), 2, "tag A should have applied + deduplicated");

    let by_cmd_x = engine.write_audit_by_command_id("cmd-x").unwrap();
    assert_eq!(by_cmd_x.len(), 2, "cmd-x should have two outcomes");

    let all = engine.write_audit_all().unwrap();
    assert!(
        all.len() >= 3,
        "expected at least three persisted records, got {}",
        all.len()
    );

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_audit_store_persists_rejected_route_errors() {
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_audit_store_reject").unwrap(),
        Box::new(WriteFactory { fail_write: false }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_audit_store_reject"),
        "Conn Write Audit Store Reject".into(),
        DriverType::new("write_audit_store_reject").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let known_tag = Tag::new(
        TagId::new("tag_known_store"),
        "Tag Known".into(),
        DeviceId::new("dev_known_store"),
        "addr:known".into(),
    );
    engine.start_connection(conn, vec![known_tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    let missing = TagId::new("tag_missing_store");
    let _ = engine
        .write_tag_with_command_id(missing.clone(), TagValue::Float(4.4), "cmd-missing".to_string())
        .await
        .expect_err("missing route must fail");
    sleep(Duration::from_millis(20)).await;

    let by_cmd = engine.write_audit_by_command_id("cmd-missing").unwrap();
    assert_eq!(by_cmd.len(), 1);
    let rec = &by_cmd[0];
    assert_eq!(rec.tag_id, missing);
    assert_eq!(
        rec.outcome,
        application::runtime::WriteCommandOutcome::Rejected
    );
    assert!(
        rec.reason
            .clone()
            .unwrap_or_default()
            .contains("tag route not found")
    );

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_audit_jsonl_repository_persists_across_restart() {
    use infrastructure::repositories::write_audit_jsonl::JsonlWriteAuditRepository;

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("ifascada_write_audit_{suffix}.jsonl"));

    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_jsonl").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let repo = JsonlWriteAuditRepository::new(&path).unwrap();
    let mut engine = RuntimeEngine::new_with_write_audit_repository(registry, Arc::new(repo));
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_jsonl"),
        "Conn Write Jsonl".into(),
        DriverType::new("write_jsonl").unwrap(),
        serde_json::json!({"command_dedup_ms": 1000}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_write_jsonl");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Write Jsonl".into(),
        DeviceId::new("dev_write_jsonl"),
        "addr:wj".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(5.0),
            "cmd-jsonl".to_string(),
        )
        .await
        .unwrap();
    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(5.0),
            "cmd-jsonl".to_string(),
        )
        .await
        .unwrap();
    sleep(Duration::from_millis(30)).await;
    engine.stop_all().await;
    drop(engine);

    let reloaded_repo = JsonlWriteAuditRepository::new(&path).unwrap();
    let persisted = reloaded_repo.by_command_id("cmd-jsonl").unwrap();
    assert_eq!(persisted.len(), 2, "expected applied + deduplicated persisted");

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn test_runtime_write_audit_query_filters_by_outcome_and_paginates() {
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_query").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_query"),
        "Conn Write Query".into(),
        DriverType::new("write_query").unwrap(),
        serde_json::json!({"command_dedup_ms": 1000}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_write_query");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Write Query".into(),
        DeviceId::new("dev_write_query"),
        "addr:wq".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    engine
        .write_tag_with_command_id(tag_id.clone(), TagValue::Float(1.0), "cmd-q".to_string())
        .await
        .unwrap();
    engine
        .write_tag_with_command_id(tag_id.clone(), TagValue::Float(1.0), "cmd-q".to_string())
        .await
        .unwrap();
    sleep(Duration::from_millis(20)).await;

    let dedup_only = engine
        .write_audit_query(&WriteAuditQuery {
            command_id: Some("cmd-q".to_string()),
            outcome: Some(WriteCommandOutcome::Deduplicated),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(dedup_only.len(), 1);
    assert_eq!(dedup_only[0].outcome, WriteCommandOutcome::Deduplicated);

    let page = engine
        .write_audit_query(&WriteAuditQuery {
            command_id: Some("cmd-q".to_string()),
            offset: 1,
            limit: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.len(), 1);

    engine.stop_all().await;
}

#[tokio::test]
async fn test_runtime_write_audit_query_filters_by_time_window() {
    let counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverType::new("write_query_time").unwrap(),
        Box::new(CountingWriteFactory {
            counts: counts.clone(),
        }),
    );

    let mut engine = RuntimeEngine::new(registry);
    let mut conn = Connection::new(
        ConnectionId::new("conn_write_query_time"),
        "Conn Write Query Time".into(),
        DriverType::new("write_query_time").unwrap(),
        serde_json::json!({}),
    );
    conn.timeout_ms = 20;

    let tag_id = TagId::new("tag_write_query_time");
    let tag = Tag::new(
        tag_id.clone(),
        "Tag Write Query Time".into(),
        DeviceId::new("dev_write_query_time"),
        "addr:wqt".into(),
    );
    engine.start_connection(conn, vec![tag]).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    let from = Utc::now();
    engine
        .write_tag_with_command_id(
            tag_id.clone(),
            TagValue::Float(2.0),
            "cmd-q-time".to_string(),
        )
        .await
        .unwrap();
    sleep(Duration::from_millis(20)).await;
    let to = Utc::now() + ChronoDuration::milliseconds(1);

    let inside = engine
        .write_audit_query(&WriteAuditQuery {
            command_id: Some("cmd-q-time".to_string()),
            from: Some(from),
            to: Some(to),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(inside.len(), 1);

    let outside = engine
        .write_audit_query(&WriteAuditQuery {
            command_id: Some("cmd-q-time".to_string()),
            to: Some(from - ChronoDuration::milliseconds(1)),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(outside.len(), 0);

    engine.stop_all().await;
}
