# REMEDIATION ROADMAP - PLAN DE IMPLEMENTACIÓN

**Duración Total**: 3 semanas (90-100 horas)
**Equipo**: 1-2 developers senior
**Branch**: `feature/security-hardening`

---

## SEMANA 1: SEGURIDAD CRÍTICA (30-40 horas)

### Day 1-2: JWT Authentication Implementation (2-3h)

#### Tareas
- [ ] Crear `crates/central-server/src/auth.rs`
- [ ] Implementar `Claims` struct + `JwtConfig`
- [ ] Crear `AuthenticatedUser` extractor
- [ ] Implementar middleware de validación

#### Files a Modificar
1. **New**: `crates/central-server/src/auth.rs` (~150 lines)
2. **Modify**: `crates/central-server/src/api.rs` (add middleware)
3. **Modify**: `crates/central-server/src/main.rs` (mod declaration)

#### Testing
```bash
# Unit test de JWT
cargo test --lib central_server::auth

# Manual test
curl -X GET http://localhost:8088/api/agents \
  -H "Authorization: Bearer <JWT_TOKEN>"
```

#### Definition of Done
- [ ] JWT generation/validation works
- [ ] Unauthorized request returns 401
- [ ] Valid token grants access
- [ ] Token expiration respected
- [ ] Scopes validation works

---

### Day 3: TLS en Conexiones (1-2h)

#### Tareas
- [ ] Cambiar `NoTls` → `SslMode::Require` en PostgreSQL
- [ ] Configurar HTTPS en REST API
- [ ] Validar certs en desarrollo

#### Files a Modificar
1. **Modify**: `crates/central-server/src/api.rs` (línea ~30)
2. **Modify**: `crates/central-server/src/main.rs` (línea ~45)
3. **Modify**: `crates/central-server/src/persistence/postgres.rs` (línea ~20)

#### Cambio Específico
```diff
- let (client, connection) = tokio_postgres::connect(dsn, NoTls).await?;
+ let mut config = dsn.parse::<Config>()?;
+ config.ssl_mode(SslMode::Require);
+ let (client, connection) = config.connect(NoTls).await?;
```

#### Testing
```bash
# Verificar TLS
openssl s_client -connect localhost:5432 -tls1_3
```

#### Definition of Done
- [ ] PostgreSQL conn uses TLS
- [ ] API serves HTTPS
- [ ] Environment vars configurados
- [ ] No más NoTls en codebase

---

### Day 4: Input Validation (1-2h)

#### Tareas
- [ ] Validar `agent_id` con regex
- [ ] Limitar tamaño de payloads
- [ ] Validar enum values
- [ ] Revisar MQTT topics

#### Cambios
```rust
// api.rs - send_command()
const MAX_PAYLOAD_SIZE: usize = 65536;  // 64KB
lazy_static::lazy_static! {
    static ref VALID_AGENT_ID: Regex =
        Regex::new(r"^[a-zA-Z0-9_-]{1,50}$").unwrap();
}

async fn send_command(
    Path(agent_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    if !VALID_AGENT_ID.is_match(&agent_id) {
        return Err(StatusCode::BAD_REQUEST);
    }

    if payload.to_string().len() > MAX_PAYLOAD_SIZE {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    // Proceed...
    Ok(...)
}
```

#### Definition of Done
- [ ] Invalid agent_id returns 400
- [ ] Oversized payload returns 413
- [ ] Validation on all endpoints
- [ ] No injection possible via inputs

---

### Day 5: Replace Crisis unwrap() Calls (5-7h)

#### Tareas
- [ ] Encontrar todos los unwrap() con clippy
- [ ] Priorizar paths críticos (auth, parsing)
- [ ] Reemplazar con `.map_err()` patterns
- [ ] Agregar recovery logic en Mutex

#### Priority Order
1. **Critical paths**: auth, command parsing, tag value parsing
2. **Database queries**: result handling
3. **MQTT message parsing**: schema validation
4. **Mutex locks**: add recovery pattern

#### Ejemplos de Reemplazo

**Antes**:
```rust
let value = input.parse::<f64>().unwrap();
```

**Después**:
```rust
let value = input.parse::<f64>()
    .map_err(|e| DomainError::ParseError(format!("{}", e)))?;
```

---

### Day 5-6: Mutex Poisoning Fixes (1-2h)

#### Tareas
- [ ] Reemplazar `.lock().unwrap()` con recovery pattern
- [ ] Manejar poisoned mutex gracefully
- [ ] Agregar logging cuando se recupera

#### Patrón de Recovry
```rust
let mut state = match self.states.lock() {
    Ok(guard) => guard,
    Err(poison) => {
        eprintln!("⚠️ Mutex was poisoned, recovering");
        poison.into_inner()
    }
};
```

#### Files Afectados
- `crates/edge-agent/src/mqtt_outbox.rs` (~5 places)
- `crates/application/tests/runtime_tests.rs` (~3 places)

---

### Day 6: Integration Testing (Full Day)

#### Manual Testing
```bash
# Test JWT validation
curl -X GET http://localhost:8088/api/agents  # 401
curl -X GET http://localhost:8088/api/agents \
  -H "Authorization: Bearer invalid" # 401
curl -X GET http://localhost:8088/api/agents \
  -H "Authorization: Bearer <valid>" # 200

# Test TLS
openssl s_client -connect db-host:5432

# Test input validation
curl -X POST http://localhost:8088/api/agents/../evil/command  # 400
curl -X POST http://localhost:8088/api/agents/edge-01/command \
  -d '{"data":"'$(python3 -c 'print("x"*70000)')'"}' # 413

# Test with invalid JSON
curl -X POST http://localhost:8088/api/agents/edge-01/command \
  -d 'invalid json' # 400, no panic
```

#### Automated Tests
```bash
cargo test --lib central_server  # All tests pass
cargo test --lib application
cargo clippy -- -D warnings  # Zero clippy warnings
```

####Definition of Done
- [ ] All manual tests pass
- [ ] 0 panics on invalid input
- [ ] CI/CD passes
- [ ] No security warnings in clippy

---

## SEMANA 2: PERFORMANCE & CONFIABILIDAD (20-25 horas)

### Day 7-8: Parallelizar Polling (2 horas)

#### Cambio
```rust
// Antes: Secuencial
for (tag_id, _) in &mut self.tags {
    self.driver.poll(&tag_id).await;
}

// Después: Paralelo
use futures::stream::{FuturesUnordered, StreamExt};

let mut futures = FuturesUnordered::new();
for (tag_id, _) in &self.tags {
    let driver = self.driver.clone();
    let tag = tag_id.clone();
    futures.push(async move {
        (tag, driver.poll(&tag).await)
    });
}

while let Some((tag_id, result)) = futures.next().await {
    match result { /* handle */ }
}
```

**Impact**: 1000 tags en ~10ms vs ~10 segundos

#### Testing
```bash
# Load test: Medir latencia antes/después
wrk -t 4 -c 100 -d 30s http://localhost:8088/api/tags
```

---

### Day 9-10: TimescaleDB Retention (1 hora)

#### Crear Migration
```sql
-- crates/central-server/migrations/0003_add_retention.sql

-- Retention: 90 días
SELECT add_retention_policy('telemetry_samples', INTERVAL '90 days');

-- Compression: 7 días (reduce storage 10x)
SELECT add_compression_policy('telemetry_samples', INTERVAL '7 days');

-- Continuous aggregates para queries rápidas
CREATE MATERIALIZED VIEW telemetry_1h AS
SELECT
    time_bucket('1 hour', ts) AS time,
    edge_id, tag_id,
    avg(value_num) AS avg_val,
    min(value_num) AS min_val,
    max(value_num) AS max_val,
    count(*) AS sample_count
FROM telemetry_samples
GROUP BY 1, 2, 3
WITH DATA;

CREATE INDEX idx_telemetry_1h_time_tag ON telemetry_1h(time DESC, tag_id);
```

#### Impact
- Antes: 500GB/mes (Disco lleno en 60 días)
- Despúes: 50GB/mes (90-day retention)

---

### Day 11: LRU Cache & Mutex Recovery (2 horas)

#### Replace HashMap
```rust
// Cargo.toml
lru = "0.12"

// Code
use lru::LruCache;
use std::num::NonZeroUsize;

pub struct ConnectionRuntime {
    seen_write_commands: LruCache<(TagId, String), Instant>,
}

impl ConnectionRuntime {
    pub fn new() -> Self {
        Self {
            seen_write_commands: LruCache::new(
                NonZeroUsize::new(10_000).unwrap()  // Max 10K
            ),
        }
    }
}
```

**Impact**: Bounded memory usage (~10MB vs unbounded)

---

### Day 12-13: Database Optimization (2 horas)

#### Agregar Indexes
```sql
-- Queries frecuentes
CREATE INDEX idx_edges_site_id ON edges(site_id);
CREATE INDEX idx_tags_edge_id ON tags(edge_id);
CREATE INDEX idx_tag_current_state_ts ON tag_current_state(ts DESC);
CREATE INDEX idx_telemetry_edge_ts ON telemetry_samples(edge_id, ts DESC);
CREATE INDEX idx_telemetry_tag_ts ON telemetry_samples(tag_id, ts DESC);
```

#### Load Testing
```bash
# Baseline
time psql -c "SELECT COUNT(*) FROM telemetry_samples WHERE edge_id = 'edge-01';"
# Before: 2.5s
# After: 50ms
```

---

## SEMANA 3: VALIDATION & HARDENING (10-15 horas)

### Day 14-15: Security Audit

#### SAST (Static Analysis)
```bash
cargo audit              # Check dependencies
cargo clippy -- -D warnings  # Code issues
cargo check             # Compilation errors
```

#### Code Review
- [ ] Revisar todas las autenticaciones
- [ ] Revisar todas las query constructions
- [ ] Revisar CORS configuration
- [ ] Revisar error messages (no leaked data)

#### Penetration Testing
```bash
# SQLi testing
curl 'http://localhost/api/tags?id=1" OR "1"="1'

# CSRF testing (confirm CORS prevents)
curl -X POST http://localhost/api/command \
  -H "Origin: https://attacker.com"  # Should reject

# SSRF testing (si hay URL parsing)
# Rate limit testing
ab -n 1000000 -c 100 http://localhost/api/tags  # Should limit
```

### Day 16: Observability

#### Add Structured Logging
```rust
use tracing::{info, warn, error};

info!("Authentication successful", user_id = ?user.id);
warn!("Invalid API token", ip = ?req.addr);
error!("Database query failed", query = ?sql);
```

#### Health Endpoints
```rust
.route("/health/live", get(health_live))
.route("/health/ready", get(health_ready))
```

### Day 17-18: Documentation & Sign-off

#### Document Changes
- [ ] Update API documentation (new auth requirements)
- [ ] Update deployment guide (TLS setup)
- [ ] Update environment variables reference
- [ ] Update security policy

#### Deployment Checklist
```
PRE-DEPLOYMENT:
  [ ] All tests passing
  [ ] All clippy warnings fixed
  [ ] Security audit completed
  [ ] Load testing done (>1000 req/s)
  [ ] Database migration tested

DEPLOYMENT:
  [ ] Backup production data
  [ ] Deploy during maintenance window
  [ ] Monitor logs for errors
  [ ] Verify JWT validation works
  [ ] Verify TLS connections

POST-DEPLOYMENT:
  [ ] Smoke tests pass
  [ ] Metrics normal
  [ ] No security warnings
  [ ] Update runbooks
```

---

## 📊 EFFORT DISTRIBUTION

```
Week 1 (32h):
  ├─ JWT Auth:               3h ████░░░░░░░░░░░░░░░░
  ├─ TLS Setup:              2h ███░░░░░░░░░░░░░░░░░
  ├─ Input Validation:       2h ███░░░░░░░░░░░░░░░░░
  ├─ unwrap() Fixes:         7h ██████████░░░░░░░░░░
  ├─ Mutex Recovery:         2h ███░░░░░░░░░░░░░░░░░
  └─ Testing:                8h ███████████░░░░░░░░░

Week 2 (22h):
  ├─ Parallel Polling:       2h ████░░░░░░░░░░░░░░░░
  ├─ TimescaleDB:            1h ██░░░░░░░░░░░░░░░░░░
  ├─ LRU Cache:              2h ████░░░░░░░░░░░░░░░░
  ├─ DB Optimization:        2h ████░░░░░░░░░░░░░░░░
  ├─ Load Testing:           5h ██████████░░░░░░░░░░
  └─ Integration:            10h ████████████████░░░░

Week 3 (16h):
  ├─ Security Audit:         6h ██████████░░░░░░░░░░
  ├─ Observability:          4h ████████░░░░░░░░░░░░
  └─ Documentation:          6h ██████████░░░░░░░░░░

TOTAL: 70h (3 developers) OR 90h (1-2 developers senior)
```

---

## 🎯 SUCCESS CRITERIA

### Security
- [ ] 0 endpoints sin autenticación
- [ ] TLS 1.3+ en todas las conexiones
- [ ] 0 hardcoded secrets en código
- [ ] 0 panics en paths críticos
- [ ] CORS restrictivo (no Any)

### Performance
- [ ] <100ms p99 latencia telemetría
- [ ] <50ms p99 latencia API query
- [ ] Polling <500ms @ 1000 tags
- [ ] API handles 1000+ req/s

### Reliability
- [ ] 99.5% uptime en testing
- [ ] 0 Mutex panics en logs
- [ ] Memory stable (<100MB growth/day)
- [ ] 90-day data retention working

---

## 📝 REFERENCES

- JWT implementation: `CODE_FIXES.md`
- General overview: `SCADA_AUDIT_REPORT.md`
- Security details: `SECURITY_FINDINGS.md`

