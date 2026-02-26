# Phase 6 Checklist (Operability and Cleanup)

## Completed in this phase
1. Crate-level docs added:
   - `crates/domain/README.md`
   - `crates/application/README.md`
   - `crates/edge-agent/README.md`
   - `crates/infrastructure/README.md`
   - `crates/launcher/README.md`
2. Architecture diagrams added:
   - `docs/scada-runtime-architecture-diagrams.md`
3. Operational runbook added:
   - `docs/runbook-core-operations.md`
4. Baseline contract suite includes config governance test:
   - `scripts/baseline-contracts.ps1`

## Pending cleanup (safe/non-disruptive recommendation)
1. Remove obsolete root docs/files only after branch freeze and final backup.
2. Prune legacy helper scripts not referenced by `docs/` or CI.
3. Consolidate duplicated environment guides into one canonical document.
