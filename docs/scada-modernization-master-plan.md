# SCADA Modernization Master Plan (Legacy-Informed, Non-Breaking)

## 1) Objective
Evolve the current SCADA stack using proven legacy ideas, without regressions in current runtime behavior.

Primary constraints:
1. Keep current MQTT contracts and API contracts stable while refactoring internals.
2. Prefer additive changes and feature flags over disruptive rewrites.
3. Allow dev/test database reset and seed profile changes to accelerate iteration.

## 2) Legacy Ideas to Reuse (and How)

### Reuse directly
1. Tag pipeline concept (`Parser -> Validator -> Scaler`) from legacy `tag_pipeline`.
2. Action executor pattern (engine emits intent, executor applies).
3. Explicit operational command handling as isolated use-case.

### Reuse with redesign
1. Printer manager idea -> generalized single-use actuator model (`on_demand` device).
2. Batch print/session workflow -> generic action workflow (`buffer + execute + persist event`).

### Do not copy as-is
1. Direct application -> infrastructure coupling (legacy `CommandListener`).
2. Infinite fixed-interval retries without governance.
3. Dropping jobs silently when transport is down.

## 3) Current Gaps to Close
1. `mqtt_bridge.rs` is oversized and mixes multiple responsibilities.
2. Tag processing logic is not formalized as reusable pipeline module.
3. Some runtime defaults (dedup/circuit) need explicit observability and governance.
4. Public API exports in `application` need stricter boundaries.

## 4) Migration Strategy (No-Break Path)
Use an incremental migration with compatibility gates:
1. Keep topic names, payload schema, and endpoint contracts unchanged during refactor.
2. Move behavior behind internal adapters/handlers first, then delete legacy paths.
3. Add contract tests before replacing old code paths.
4. Roll out by phase; each phase must be releasable.

## 5) Phased Implementation Plan

## Phase 0 - Baseline and Safety Gates
Goal: freeze expected behavior and establish measurable non-regression.

Deliverables:
1. Contract tests for:
   - MQTT command/action/result/audit topics.
   - API current-state endpoints.
   - SSE event stream filtering.
2. Golden E2E scripts for:
   - serial scale path,
   - modbus path,
   - action workflow path.
3. Single-command baseline check:
   - `powershell -ExecutionPolicy Bypass -File scripts/baseline-contracts.ps1`

Exit criteria:
1. Baseline tests pass before any refactor.

## Phase 1 - Split `mqtt_bridge` into Handlers
Goal: remove god-object risk without changing behavior.

Deliverables:
1. Internal modules:
   - `write_command_handler`
   - `action_command_handler`
   - `health_handler`
   - `config_handler`
   - `alert_handler`
   - `state_publish_handler`
2. `mqtt_bridge` keeps orchestration only.

Exit criteria:
1. No topic or payload changes.
2. Existing edge-agent tests remain green.

## Phase 2 - Formal Tag Processing Pipeline
Goal: introduce reusable application-layer pipeline inspired by legacy.

Deliverables:
1. `application::runtime::tag_pipeline` with:
   - parse,
   - validate,
   - scale/transform,
   - display format policy.
2. ConnectionRuntime integration by policy from `tag.metadata_json`.

Exit criteria:
1. Existing tags keep current behavior by default.
2. New pipeline-enabled tags work via metadata only.

## Phase 3 - Generic Action Orchestrator
Goal: keep automation generic and executable at edge/central scope.

Deliverables:
1. `ActionRequest -> ActionExecutor` internal port contract.
2. Built-in executors:
   - `print.escpos`
   - `buffer.weights.accumulate`
   - `connection.check`
3. Deterministic idempotency and outcome audit policy.

Exit criteria:
1. Existing automation flows remain compatible.
2. Action observability complete (`result`, `audit`, `device/conn/state` when applicable).

## Phase 4 - On-Demand Device Model
Goal: support non-telemetry devices (actuators) with reliable status semantics.

Deliverables:
1. `status_policy.mode = on_demand` fully documented and tested.
2. Optional `probe_on_start` and manual `connection.check` flows.
3. Device lamps driven by operational connectivity events, not telemetry.

Exit criteria:
1. Single-use devices can be monitored without historian spam.

## Phase 5 - Configuration Governance from Central DB
Goal: central DB remains source of truth; edge cache for offline only.

Deliverables:
1. Signed config pull/apply lifecycle hardened.
2. Deterministic config hash comparison and apply receipt.
3. Dev/admin workflows for seed profiles and environment portability.

Exit criteria:
1. Edge boots from central or local signed cache with deterministic behavior.

## Phase 6 - Operability and Cleanup
Goal: remove accumulated debt and stabilize long-term operation.

Deliverables:
1. Remove dead files/residual artifacts.
2. Add crate-level docs and architecture diagrams.
3. Add operational runbooks (failure policy, replay, recovery).

Exit criteria:
1. Observability and runbook coverage complete for core flows.

## 6) DB Reset and Seed Policy (Dev/Test)
Use seed profiles to test focused scenarios quickly.

Profiles:
1. `minimal`:
   - 3 edges (serial, modbus, simulator),
   - minimal deterministic catalog for manual/e2e tests.
2. `sim20`:
   - multi-area simulation profile.
3. `full`:
   - legacy-like expanded dev seed.

Command:
```powershell
powershell -ExecutionPolicy Bypass -File scripts/reset-db-and-seed.ps1 -PgDsn "$env:CENTRAL_PG_DSN" -ResetSchema -SeedProfile minimal
```

## 7) Immediate Sprint Backlog (Next 2 Iterations)
Iteration A:
1. Phase 0 baseline contracts.
2. Phase 1 handler split skeleton.
3. Phase 2 pipeline interfaces (no behavior change yet).

Iteration B:
1. Phase 2 pipeline activation for selected tags.
2. Phase 3 action-orchestrator extraction.
3. Phase 4 on-demand device probes and checks hardening.

## 8) Definition of Done per Change
1. Unit tests for domain/application logic.
2. Integration test for edge-central flow when applicable.
3. Updated documentation in `docs/`.
4. No contract regressions in MQTT/API/SSE.
