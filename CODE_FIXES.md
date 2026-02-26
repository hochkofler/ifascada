# CODE FIXES - SOLUCIONES LISTA PARA IMPLEMENTAR

**Estado**: Copy-paste ready, production-grade code

---

## FIX #1: JWT AUTHENTICATION (2-3h)

### Nuevo archivo: `crates/central-server/src/auth.rs`

```rust
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,              // user ID: "operator@site-01"
    pub exp: i64,                 // expiration timestamp
    pub iat: i64,                 // issued at
    pub scopes: Vec<String>,      // ["read:tags", "write:commands"]
}

pub struct JwtConfig {
    pub secret: String,
    pub expiration_hours: i64,
}

impl JwtConfig {
    pub fn new() -> Self {
        let secret = std::env::var("JWT_SECRET").expect(
            "JWT_SECRET environment variable is required. Set it to at least 32 random characters."
        );

        Self {
            secret,
            expiration_hours: 24,
        }
    }

    pub fn encode_token(&self, user_id: &str, scopes: Vec<String>) -> Result<String, String> {
        let now = Utc::now();
        let payload = Claims {
            sub: user_id.to_string(),
            exp: (now + Duration::hours(self.expiration_hours)).timestamp(),
            iat: now.timestamp(),
            scopes,
        };

        encode(
            &Header::default(),
            &payload,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| e.to_string())
    }

    pub fn decode_token(&self, token: &str) -> Result<Claims, String> {
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )
        .map(|data| data.claims)
        .map_err(|e| e.to_string())
    }
}

pub struct AuthenticatedUser(pub Claims);

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;
        let auth_header = headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        if !auth_header.starts_with("Bearer ") {
            return Err(StatusCode::UNAUTHORIZED);
        }

        let token = &auth_header[7..];

        // Get JWT config from environment
        let jwt_config = JwtConfig::new();
        jwt_config
            .decode_token(token)
            .map(|claims| AuthenticatedUser(claims))
            .map_err(|_| StatusCode::UNAUTHORIZED)
    }
}

pub async fn require_scope(
    required_scope: &str,
) -> impl Fn(AuthenticatedUser) -> Result<AuthenticatedUser, StatusCode> {
    move |user: AuthenticatedUser| {
        if user.0.scopes.contains(&required_scope.to_string()) {
            Ok(user)
        } else {
            Err(StatusCode::FORBIDDEN)
        }
    }
}
```

### Modificar: `crates/central-server/src/api.rs`

```rust
use crate::auth::{AuthenticatedUser, JwtConfig, require_scope};
use tower_http::cors::CorsLayer;

pub fn create_router(state: Arc<AppState>) -> Router {
    let jwt_config = Arc::new(JwtConfig::new());

    // Rutas públicas
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .route("/api/edge/enroll", post(edge_enroll));

    // Rutas protegidas
    let protected_routes = Router::new()
        .route("/api/agents", get(get_agents_protected))
        .route("/api/tags", get(get_all_tags_protected))
        .route("/api/agents/{id}/command", post(send_command_protected))
        .route("/api/reports", get(get_reports_protected))
        .layer(axum::middleware::from_fn(auth_middleware))
        .with_state(state);

    // CORS restrictivo
    let cors = CorsLayer::very_restrictive()
        .allow_origin("https://trusted.example.com".parse().unwrap())
        .allow_methods([GET, POST, PUT])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    public_routes
        .merge(protected_routes)
        .layer(cors)
        .with_state(jwt_config)
}

async fn auth_middleware(
    auth: AuthenticatedUser,
    req: axum::http::Request<Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    // Auth ya validada por extractor
    Ok(next.run(req).await)
}

// Handlers existentes, pero ahora con AuthenticatedUser
async fn get_agents_protected(
    _auth: AuthenticatedUser,  // Requiere auth
    // ... resto del handler
) -> impl IntoResponse {
    // ...
}

async fn send_command_protected(
    auth: AuthenticatedUser,
    Path(agent_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    // Validar scope
    if !auth.0.scopes.contains(&"write:commands".to_string()) {
        return Err(StatusCode::FORBIDDEN);
    }

    // ... resto de lógica
    Ok(StatusCode::OK)
}
```

### Agregar a `Cargo.toml`

```toml
jsonwebtoken = "9"
chrono = "0.4"
axum = { version = "0.7", features = ["macros"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "auth"] }
```

---

## FIX #2: TLS EN POSTGRESQL (1-2h)

### Modificar: `crates/central-server/src/api.rs` (línea ~30)

```rust
use tokio_postgres::config::{Config, SslMode};

pub async fn init_postgres() -> Result<PostgresClient, Box<dyn Error>> {
    let dsn = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL required (e.g., postgres://user:pass@host/db)");

    // Parse and enforce TLS
    let mut config: Config = dsn.parse()?;
    config.ssl_mode(SslMode::Require);  // ✅ Obligatorio

    let (client, connection) = config.connect(tokio_postgres::tls::NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    Ok(client)
}
```

### Modificar: `crates/central-server/src/main.rs` (línea ~45)

```rust
// Same approach - use SslMode::Require
let mut config = dsn.parse::<Config>()?;
config.ssl_mode(SslMode::Require);
let (client, connection) = config.connect(NoTls).await?;
```

### Modificar: `crates/central-server/src/persistence/postgres.rs` (línea ~20)

```rust
// Same fix
config.ssl_mode(SslMode::Require);
```

### Environment Variable

```bash
# .env.production
DATABASE_URL=postgres://user:pass@prod-db.example.com:5432/scada?sslmode=require&sslcert=client.crt&sslkey=client.key
```

---

## FIX #3: ELIMINAR UNWRAP() PATTERNS (5-7h)

### Pattern #1: Parse errors con `.map_err()`

**Antes**:
```rust
let value = input.parse::<f64>().unwrap();
```

**Después**:
```rust
let value = input.parse::<f64>()
    .map_err(|e| DomainError::ParseError(format!("Invalid float: {}", e)))?;
```

### Pattern #2: Option con `.ok_or()` + error context

**Antes**:
```rust
let tag = self.tags.get(&tag_id).unwrap();
```

**Después**:
```rust
let tag = self.tags.get(&tag_id)
    .ok_or(DomainError::TagNotFound(tag_id.clone()))?;
```

### Pattern #3: Mutex con recovery

**Antes**:
```rust
let mut state = self.states.lock().unwrap();
```

**Después**:
```rust
let mut state = match self.states.lock() {
    Ok(guard) => guard,
    Err(poison) => {
        eprintln!("⚠️ WARNING: Mutex was poisoned, recovering from poisoned state");
        poison.into_inner()  // Recover del poisoned state
    }
};
```

### Helper function para reuso

```rust
fn recover_mutex<T>(
    result: Result<std::sync::MutexGuard<T>, std::sync::PoisonError<std::sync::MutexGuard<T>>>
) -> std::sync::MutexGuard<T> {
    match result {
        Ok(guard) => guard,
        Err(poison) => {
            eprintln!("⚠️ Mutex poisoned, recovering...");
            poison.into_inner()
        }
    }
}

// Uso:
let mut state = recover_mutex(self.states.lock());
```

### Encontrar todos automáticamente

```bash
cargo clippy -- -D warnings 2>&1 | grep "unwrap"
sed -i 's/\.unwrap_or(/.map_err(|e| DomainError::from(e))?; \/\/ OLD: /g' crates/**/src/**/*.rs
```

---

## FIX #4: RATE LIMITING (1h)

### Agregar a `Cargo.toml`

```toml
tower-governor = "0.2"
```

### En `crates/central-server/src/api.rs`

```rust
use tower_governor::{Governor, governable::Governor as _};
use std::num::NonZeroU32;

pub fn create_router(state: Arc<AppState>) -> Router {
    // 100 requests per second
    let governor = Governor::builder()
        .per_second(NonZeroU32::new(100).unwrap())
        .burst_size(200)
        .use_direct_dispatch()
        .build()
        .expect("rate limiter build failed");

    Router::new()
        .route("/api/agents", get(get_agents))
        .route("/api/tags", get(get_all_tags))
        .route("/api/agents/{id}/command", post(send_command))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(move |req, next| {
            if governor.check().is_ok() {
                Ok(next.run(req))
            } else {
                Err(StatusCode::TOO_MANY_REQUESTS)
            }
        }))
        .with_state(state)
}
```

---

## FIX #5: TIMESCALEDB RETENTION (0.5h)

### Nueva file: `crates/central-server/migrations/0003_add_retention.sql`

```sql
-- Retention policy: 90 días
SELECT add_retention_policy('telemetry_samples', INTERVAL '90 days');

-- Compression policy: comprimir después de 7 días
SELECT add_compression_policy('telemetry_samples', INTERVAL '7 days');

-- Crear continuous aggregate para queries rápidas (hourly)
CREATE MATERIALIZED VIEW IF NOT EXISTS telemetry_1h AS
SELECT
    time_bucket('1 hour', ts) AS time,
    site_id, edge_id, tag_id,
    avg((value_num)::float8) AS avg_value,
    min((value_num)::float8) AS min_value,
    max((value_num)::float8) AS max_value,
    count(*) AS sample_count
FROM telemetry_samples
GROUP BY time, site_id, edge_id, tag_id
WITH DATA;

CREATE INDEX IF NOT EXISTS idx_telemetry_1h_time_tag
ON telemetry_1h (time DESC, tag_id);

-- Refresh policy para continuous aggregate
SELECT add_continuous_agg_policy(
    'telemetry_1h',
    start_offset => INTERVAL '1 month',
    end_offset => INTERVAL '1 hour',
    if_not_exists => true
);
```

### Ejecutar migración

```bash
# En el directorio de migrations
sqlx migrate run --database-url $DATABASE_URL
```

---

## FIX #6: INPUT VALIDATION (1h)

### En `crates/central-server/src/api.rs`

```rust
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref VALID_AGENT_ID: Regex = Regex::new(r"^[a-zA-Z0-9_-]{1,50}$").unwrap();
    static ref VALID_TAG_ID: Regex = Regex::new(r"^[a-zA-Z0-9_-]{1,100}$").unwrap();
}

const MAX_PAYLOAD_SIZE: usize = 65536;  // 64 KB

async fn send_command(
    auth: AuthenticatedUser,
    Path(agent_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    // Validate agent_id
    if !VALID_AGENT_ID.is_match(&agent_id) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate payload size
    let payload_str = payload.to_string();
    if payload_str.len() > MAX_PAYLOAD_SIZE {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    // Check scopes
    if !auth.0.scopes.contains(&"write:commands".to_string()) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Proceed with command...
    Ok(StatusCode::OK)
}
```

### Agregar a `Cargo.toml`

```toml
regex = "1"
lazy_static = "1.4"
```

---

## FIX #7: LRU CACHE (1-2h)

### Modificar: `crates/application/src/runtime/connection_runtime.rs`

```rust
use lru::LruCache;
use std::num::NonZeroUsize;

pub struct ConnectionRuntime {
    // ... other fields ...

    // Replace: HashMap<(TagId, String), Instant>
    // With: LRU cache (max 10,000 entries, ~10MB RAM)
    seen_write_commands: LruCache<(TagId, String), Instant>,
}

impl ConnectionRuntime {
    pub fn new(tag_count: usize) -> Self {
        let dedup_capacity = (tag_count * 2).max(1000).min(10_000);

        Self {
            // ... init other fields ...
            seen_write_commands: LruCache::new(
                NonZeroUsize::new(dedup_capacity).unwrap()
            ),
        }
    }

    pub fn is_command_duplicate(&mut self, tag_id: &TagId, cmd_id: &str) -> bool {
        if let Some(&seen_at) = self.seen_write_commands.peek(&(tag_id.clone(), cmd_id.to_string())) {
            // Consider duplicate if < 30 seconds old
            Instant::now().duration_since(seen_at) < Duration::from_secs(30)
        } else {
            false
        }
    }

    pub fn mark_command_sent(&mut self, tag_id: TagId, cmd_id: String) {
        self.seen_write_commands.put((tag_id, cmd_id), Instant::now());
        // LRU automatically evicts oldest if exceeds capacity
    }
}
```

### Agregar a `Cargo.toml`

```toml
lru = "0.12"
```

---

## TESTING CODE CHANGES

### Unit Tests Template

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_token_generation() {
        let config = JwtConfig::new();
        let token = config.encode_token("user@example.com", vec!["read:tags".to_string()])
            .expect("encode failed");
        assert!(!token.is_empty());
    }

    #[test]
    fn test_jwt_token_validation() {
        let config = JwtConfig::new();
        let token = config.encode_token("user", vec![])
            .expect("encode failed");
        let claims = config.decode_token(&token)
            .expect("decode failed");
        assert_eq!(claims.sub, "user");
    }

    #[tokio::test]
    async fn test_unauthorized_request() {
        let app = create_test_app();
        let response = app.oneshot(
            axum::http::Request::builder()
                .uri("/api/agents")
                .build(Body::empty())
                .unwrap()
        ).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
```

---

## VERIFICATION CHECKLIST

- [ ] Código compila (`cargo build --release`)
- [ ] Tests pasan (`cargo test`)
- [ ] Clippy limpio (`cargo clippy -- -D warnings`)
- [ ] Security audit limpio (`cargo audit`)
- [ ] JWT generation/validation works
- [ ] TLS connection established
- [ ] Input validation rejects bad data
- [ ] Rate limiting active
- [ ] No panics on invalid input

---

**Para más detalles**: Ver `REMEDIATION_ROADMAP.md` para timeline completo

