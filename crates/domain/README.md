# domain crate

## Purpose
Core business model and policies for SCADA runtime and central processing.

## Owns
1. Entities/value objects (`tag`, `connection`, `device`, `id`).
2. Domain policies (`device/status`, `tag/status`, `tag/quality`).
3. Pure contracts and invariants (no transport/database dependencies).

## Boundaries
1. Must not depend on infrastructure adapters.
2. Must stay deterministic and side-effect free.

## Validation
```powershell
cargo test -q -p domain
```
