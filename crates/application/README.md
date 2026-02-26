# application crate

## Purpose
Use-case orchestration layer between domain and adapters.

## Owns
1. Runtime execution flows (`runtime/*`).
2. Automation evaluation and action intent generation.
3. Policy application over domain models.

## Boundaries
1. Depends on `domain`.
2. Exposes ports/interfaces to be implemented by edge/central adapters.
3. Must not include transport/protocol-specific I/O code.

## Validation
```powershell
cargo test -q -p application
```
