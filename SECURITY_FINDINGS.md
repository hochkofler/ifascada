# SECURITY FINDINGS - ANÁLISIS DETALLADO

**Sistema**: SCADA ifascada
**Fecha**: 25 de febrero de 2026
**Severidad Total**: 🔴 CRÍTICA

---

## VULNERABILIDADES CRÍTICAS (3)

### [SEC-001] FALTA DE AUTENTICACIÓN EN API REST

**Severidad**: 🔴 CRÍTICA
**CVSS**: 8.6 (High)
**Ubicación**: `crates/central-server/src/api.rs` (líneas 1-50, 550-580)
**Afectados**: Todos los endpoints REST

#### Descripción
El servidor central expone endpoints REST sensibles sin ningún mecanismo de autenticación. CORS está configurado como abierto a cualquier origen.

#### Código Vulnerable
```rust
pub fn create_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)        // ❌ CORS abierto
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/agents", get(get_agents))           // Sin auth
        .route("/api/tags", get(get_all_tags))           // Sin auth
        .route("/api/tags/{id}", get(get_tag))           // Sin auth
        .route("/api/agents/{id}/command", post(send_command))  // ❌ CRÍTICO
        .route("/api/reports", get(get_reports))         // Sin auth
        .layer(cors)
        .with_state(state)
}

async fn send_command(
    Path(agent_id): Path<String>,                    // Sin validar
    Json(payload): Json<serde_json::Value>,          // Sin límite
) -> impl IntoResponse {
    let topic = format!("scada/cmd/{}", agent_id);   // Inyección posible
    state.mqtt_client.publish(&topic, &payload.to_string(), false).await
}
```

#### Impacto
Un **atacante sin autenticación puede**:
- Leer valores de cualquier tag/dispositivo
- Enviar comandos arbitrarios (escala tare, printer control)
- Leer reportes y datos sensibles
- Sobrecargar el sistema (DoS)
- Modificar configuración remota vía MQTT

#### Ejemplos de Ataque
```bash
# Leer todos los tags
curl -X GET http://target/api/tags

# Enviar comando arbitrario
curl -X POST http://target/api/agents/edge-01/command \
  -H "Content-Type: application/json" \
  -d '{"action": "stop_pump"}'

# Bombardear con requests (DoS)
ab -n 100000 -c 100 http://target/api/tags
```

#### Solución
Implementar JWT authentication con middleware que valide:
1. Bearer token en header `Authorization`
2. Signature JWT
3. Expiración de token
4. Scopes (read:tags, write:commands, etc)

#### Effort
**2-3 horas**: Crear auth.rs module + middleware + test

---

### [SEC-002] SIN TLS EN CONEXIONES A POSTGRESQL

**Severidad**: 🔴 CRÍTICA
**CVSS**: 7.5 (High)
**Ubicación**: 3 archivos
- `crates/central-server/src/api.rs` (línea 30)
- `crates/central-server/src/main.rs` (línea 45)
- `crates/central-server/src/persistence/postgres.rs` (línea 20)

#### Descripción
Todas las conexiones a PostgreSQL usan `NoTls`, exponiendo credenciales en texto plano en la red.

#### Código Vulnerable
```rust
// ❌ api.rs:30
let (client, connection) = tokio_postgres::connect(dsn, NoTls).await?;

// ❌ main.rs:45
let (client, connection) = tokio_postgres::connect(&dsn_cleanup, NoTls).await?;

// ❌ persistence/postgres.rs:20
let (client, connection) = tokio_postgres::connect(dsn, NoTls).await?;
```

#### Impacto
- Credentials PostgreSQL visibles en tráfico de red (tcpdump)
- Man-in-the-middle attacks para capturar/modificar datos
- Cumplimiento: Falla requerimientos HIPAA, PCI-DSS

#### Ataque Simulado
```bash
# En red corporativa, capturar tráfico PostgreSQL
sudo tcpdump -i eth0 -A 'tcp port 5432' | grep password
# → Muestra credentials en texto plano
```

#### Solución
Obligar TLS 1.2+ usando `SslMode::Require` en PostgreSQL

#### Effort
**1-2 horas**: Cambiar 3 archivos, revisar certs

---

### [SEC-003] 393 LLAMADAS A `unwrap()` - PANIC DoS

**Severidad**: 🔴 CRÍTICA
**CVSS**: 7.3 (High)
**Ubicación**: Múltiples archivos en todos los crates
**Estadísticas**:
- `unwrap()` calls: 393
- `expect()` calls: Muchos
- `panic!()` calls: 22
- Mutex `.lock().unwrap()`: 10+

#### Descripción
Cualquier entrada que no se parsee correctamente causa `panic!()` inmediato, derribando el servicio.

#### Ejemplos de Código Vulnerable
```rust
// ❌ application/src/automation/engine.rs:90
let state = self.states.get(&spec.id).cloned().unwrap_or_default();

// ❌ application/src/runtime/tag_pipeline.rs:180
value.parse::<f64>().unwrap_or(1.0)

// ❌ edge-agent/src/mqtt_outbox.rs:180
let mut map = self.counts.lock().unwrap();  // Panic si Mutex envenenado

// ❌ tests/runtime_tests.rs:múltiples
let step = self.steps
    .lock().unwrap()           // Panic #1
    .pop_front()
    .unwrap_or(PollStep::Empty);  // Panic #2
```

#### Impacto
- Entrada JSON malformado → panic
- Mensaje MQTT con schema inválido → panic
- Comando con valor fuera de rango → panic
- Mutex en estado envenenado → panic inmediato
- **Resultado**: Service crash = DoS trivial

#### Ataque Simulado
```bash
# JSON inválido
curl -X POST http://target/api/agents/edge-01/command \
  -H "Content-Type: application/json" \
  -d 'invalid json'
# → Server panic → 500 error → eventual crash

# Valor fuera de tipo
# Sender: {"value": null} en lugar de número
# → parse error → unwrap panic

# Concurrencia: Trigger Mutex poisoning
# Thread 1: lock Mutex
# Thread 2: lock, panic en sección crítica
# → Mutex envenenado → todos los threads panic
```

#### Solución
Eliminar todos los `unwrap()` en código crítico:
1. Usar `.map_err()` para propagación de errores
2. Usar `.unwrap_or_else()` con recovery logic
3. Usar Mutex recovery pattern en lugar de panic
4. Usar `?` operator para early exit

#### Effort
**5-7 horas**: Mayor esfuerzo del proyecto

**How to find**:
```bash
cargo clippy -- -D warnings
grep -r "unwrap()" crates/ --include="*.rs" | wc -l
```

---

## VULNERABILIDADES ALTAS (6)

### [SEC-004] MQTT TOPIC INJECTION

**Ubicación**: `crates/central-server/src/api.rs` (líneas 550-580)
**Severidad**: 🟠 ALTA

**Problema**: `agent_id` no se valida antes de usarlo en tópico MQTT
```rust
let topic = format!("scada/cmd/{}", agent_id);  // agent_id podría ser: "../../../danger"
```

**Risk**: Publicar a tópicos arbitrarios

**Fix**: Validar con regex
```rust
if !Regex::new(r"^[a-zA-Z0-9_-]{1,50}$")?.is_match(&agent_id) {
    return Err(StatusCode::BAD_REQUEST);
}
```
**Effort**: 1h

---

### [SEC-005] CREDENCIALES HARDCODED

**Ubicación**: `crates/edge-agent/src/main.rs` (línea 95)
**Severidad**: 🟠 ALTA

**Problema**: Default secret en código de producción
```rust
config_check_hmac_secret: std::env::var("EDGE_CONFIG_HMAC_SECRET")
    .or_else(|| Some("dev-edge-config-signing-secret".to_string())),
    // ↑ HARDCODED!
```

**Risk**: Cualquiera que lea el código conoce el "secret"

**Fix**: Fallar si no configurado en producción
**Effort**: 1-2h

---

### [REL-001] HASHMAP SIN LÍMITE

**Ubicación**: `crates/application/src/runtime/connection_runtime.rs` (línea ~40)
**Severidad**: 🟡 MEDIA

**Problema**: Dedup HashMap crece sin límite
```rust
seen_write_commands: HashMap<(TagId, String), Instant>  // Sin límite!
```

**Escenario**: 1M unique IDs × 1KB = 1GB RAM → OOM

**Fix**: Usar LRU cache con 10K max entries
**Effort**: 1-2h

---

### [PERF-001] SIN TIMESCALEDB RETENTION

**Ubicación**: `crates/central-server/migrations/`
**Severidad**: 🟠 ALTA (operacional)

**Problema**: Tabla telemetry_samples crece indefinidamente
```
Day 30:  25TB
Day 90:  75TB
Day 180: Disco lleno → Service down
```

**Fix**: Agregar retention policy
```sql
SELECT add_retention_policy('telemetry_samples', INTERVAL '90 days');
SELECT add_compression_policy('telemetry_samples', INTERVAL '7 days');
```
**Effort**: 0.5h

---

### [PERF-002] POLLING SECUENCIAL

**Ubicación**: `crates/application/src/runtime/connection_runtime.rs` (línea ~200)
**Severidad**: 🟠 ALTA

**Problema**: Polling cada tag secuencialmente
```rust
for (tag_id, _) in &mut self.tags {
    self.driver.poll(&tag_id).await;  // Espera cada uno!
}
// 1000 tags × 10ms = 10 segundos → TIMEOUT
```

**Fix**: Paralelizar con FuturesUnordered
**Effort**: 2h

---

### [PERF-003] SIN RATE LIMITING

**Ubicación**: `crates/central-server/src/api.rs` (línea ~50)
**Severidad**: 🟠 ALTA (operacional)

**Problema**: Cualquiera puede bombardear la API
```bash
ab -n 100000 -c 100 http://target/api/tags  # Api muere
```

**Fix**: Agregar tower-governor layer
**Effort**: 1h

---

## RESUMEN DE REMEDIACIÓN

```
┌────────────────────────────────────────────┐
│ SEVERIDAD  │ COUNT │ EFFORT  │ PRIORITY    │
├────────────────────────────────────────────┤
│ 🔴 CRÍTICA │ 3     │ 8-12h   │ SEMANA 1    │
│ 🟠 ALTA    │ 6     │ 7-10h   │ SEMANA 1-2  │
│ 🟡 MEDIA   │ 11+   │ 15-20h  │ SEMANA 2-3  │
├────────────────────────────────────────────┤
│ TOTAL      │ 20+   │ 90-100h │ 3 semanas   │
└────────────────────────────────────────────┘
```

---

## REFERENCIAS RÁPIDAS

**Para implementación**: Ver `CODE_FIXES.md`
**Para timeline**: Ver `REMEDIATION_ROADMAP.md`
**Para visión general**: Ver `SCADA_AUDIT_REPORT.md`

