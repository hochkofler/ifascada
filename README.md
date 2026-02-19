# IFA SCADA

Tag-based SCADA system for industrial automation with centralized configuration and monitoring.

## Architecture

This project follows **Hexagonal Architecture** (Ports & Adapters) with **Domain-Driven Design**:

```
┌─────────────────────────────────────────┐
│         Central Server (API)            │
│   ┌─────────────────────────────┐      │
│   │   Application Layer         │      │
│   │  (Use Cases / Services)     │      │
│   └──────────┬──────────────────┘      │
│              │                          │
│   ┌──────────▼──────────────────┐      │
│   │     Domain Layer            │      │
│   │  (Entities, Value Objects)  │      │
│   │   NO EXTERNAL DEPENDENCIES  │      │
│   └──────────┬──────────────────┘      │
│              │                          │
│   ┌──────────▼──────────────────┐      │
│   │   Infrastructure Layer      │      │
│   │  (DB, Drivers, Messaging)   │      │
│   └─────────────────────────────┘      │
└─────────────────────────────────────────┘
```

## Project Structure

- **`crates/domain`** - Pure domain logic, no external dependencies
- **`crates/application`** - Use cases and business workflows
- **`crates/infrastructure`** - Database, drivers, external integrations
- **`crates/central-server`** - REST API and orchestration
- **`crates/edge-agent`** - Generic tag executor

## Key Concepts

### Tag
A **tag** is a data point representing a physical or logical variable:
- Example: `SCALE_CABINA_1` → peso + unidad + estabilidad

### Driver
A **driver** reads/writes tag values from/to devices:
- RS232, Modbus, OPC-UA, HTTP

### Update Modes
- **OnChange**: Event-driven (scales report when data arrives)
- **Polling**: Periodic reading (sensors read every N seconds)
- **PollingOnChange**: Hybrid (poll but only report significant changes)

## Development Principles

✅ **TDD (Test-Driven Development)**: Tests written before implementation  
✅ **SOLID Principles**: Clean, maintainable code  
✅ **DDD (Domain-Driven Design)**: Domain at the center  
✅ **Hexagonal Architecture**: Decoupled layers  

## Getting Started

### Prerequisites
- Rust 1.75+
- PostgreSQL 14+

### Build
```bash
cargo build
```

### Test
```bash
cargo test
```

### Run Central Server
```bash
cargo run -p central-server
```

### Run Edge Agent
```bash
cargo run -p edge-agent
```

## License

MIT
