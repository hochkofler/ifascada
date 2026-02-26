# infrastructure crate

## Purpose
Concrete adapters for protocols, repositories, and persistence helpers.

## Responsibilities
1. Driver implementations (Modbus RTU/TCP, Serial ASCII, simulator).
2. Repository implementations used by application/central layers.
3. I/O specific concerns isolated from domain policies.

## Boundaries
1. May depend on external libraries and I/O stacks.
2. Must not define business rules already present in `domain`.

## Validation
```powershell
cargo test -q -p infrastructure
```
