# SerialAscii Configurable Multi-Field Parser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `SerialAscii` so a configured block parser can emit multiple standard compound tag values while preserving every existing scale configuration.

**Architecture:** Keep serial transport and the per-tag pipeline unchanged. Add parser version 2 and block framing inside `SerialAscii`; compiled field definitions extract typed `value`, captured `unit`, and matched `raw`, then `tag_map` routes each compound envelope to a tag. Legacy parser version 1 remains the default path.

**Tech Stack:** Rust, Tokio serial, `regex`, Serde JSON, existing `DriverConnection` multi-tag contract, existing metadata tag pipeline.

## Global Constraints

- Legacy `SerialAscii` line, regex, `scale:*`, and compound behavior must remain byte-for-byte compatible.
- Parser version 2 field types are exactly `float`, `integer`, `string`, and `boolean`; `float` is the default.
- Version 2 sources are `field:<name>` plus reserved `raw`.
- Each parsed field emits the existing JSON keys `value`, `unit`, and `raw` so `compound_json` pipeline extraction requires no changes.
- Missing optional fields emit no update; missing required fields emit a tag-level error without suppressing valid siblings.
- No product version bump, database migration, MQTT contract change, or new device-specific driver.

---

### Task 1: Configurable Field Parser and Output Routing

**Files:**
- Modify: `crates/infrastructure/src/drivers/serial_ascii.rs`
- Test: `crates/infrastructure/src/drivers/serial_ascii.rs` test module

**Interfaces:**
- Consumes: `ParserConfig`, `tag_map`, `TagValue`, and the existing compound JSON envelope.
- Produces: `CompiledParser::Fields`, `CompiledField`, `SerialOutputMode::Field(String)`, and `map_frame_to_outputs(...) -> Vec<(TagId, Result<TagValue, DomainError>)>`.

- [ ] **Step 1: Write failing tests for typed fields and multi-tag routing**

Add tests using one frame containing `93.79mV`, `5.25pH`, `24.2c`, and `98.82%`. Configure four field regexes with `value_group = 1`, `unit_group = 2`, and `value_type = float`. Assert each `field:<name>` tag receives its own compound JSON with the expected `value`, `unit`, and matched-line `raw`.

```rust
assert_eq!(compound(&out, "tag_ph")["value"], 5.25);
assert_eq!(compound(&out, "tag_ph")["unit"], "pH");
assert_eq!(compound(&out, "tag_mv")["value"], 93.79);
assert_eq!(compound(&out, "tag_temperature")["value"], 24.2);
assert_eq!(compound(&out, "tag_efficiency")["value"], 98.82);
```

- [ ] **Step 2: Run the field tests and verify RED**

Run: `cargo test -p infrastructure serial_ascii::tests::test_configurable_fields -- --nocapture`

Expected: compilation failure or assertion failure because `fields`, `field:*`, and frame mapping are not implemented.

- [ ] **Step 3: Add versioned parser configuration and compilation**

Extend `ParserConfig` without changing legacy defaults:

```rust
#[derive(Debug, Clone, Deserialize)]
struct ParserConfig {
    #[serde(default = "default_parser_version")]
    version: u8,
    #[serde(default)]
    fields: HashMap<String, FieldConfig>,
    // existing regex/sign/value/unit members remain
}

#[derive(Debug, Clone, Deserialize)]
struct FieldConfig {
    regex: String,
    #[serde(default = "default_value_group")]
    value_group: usize,
    #[serde(default)]
    unit_group: Option<usize>,
    #[serde(default = "default_field_value_type")]
    value_type: String,
    #[serde(default = "default_required")]
    required: bool,
}
```

Compile and validate all regexes, value types, capture groups, and mapped field names during `from_connection`. Version 1 uses the existing `parse_scale_line`; version 2 requires at least one field.

- [ ] **Step 4: Implement field extraction and compound envelopes**

For each mapped field, capture and convert its configured value. Capture `unit` when configured, otherwise use an empty string. Use the complete regex match, trimmed, as that field's `raw`. Build the standard envelope with `serde_json::json!` and return a `TagValue::String`.

Required missing or conversion failures return `Err(DomainError::DriverError(...))` for the mapped tag. Optional missing fields are omitted. Valid sibling fields remain in the result vector. Reserved `raw` returns the complete frame.

- [ ] **Step 5: Add validation tests**

Cover unsupported parser version, empty version 2 fields, invalid regex, unknown `field:<name>` mapping, unsupported value type, invalid capture group, optional missing field, required missing field, integer/string/boolean conversion, and raw frame output.

- [ ] **Step 6: Run focused tests and verify GREEN**

Run: `cargo test -p infrastructure serial_ascii -- --nocapture`

Expected: all legacy and new serial tests pass.

- [ ] **Step 7: Commit parser functionality**

```bash
git add crates/infrastructure/src/drivers/serial_ascii.rs
git commit -m "feat(serial): add configurable multi-field parser"
```

### Task 2: Block Framing and Fragmented Serial Reads

**Files:**
- Modify: `crates/infrastructure/src/drivers/serial_ascii.rs`
- Test: `crates/infrastructure/src/drivers/serial_ascii.rs` test module

**Interfaces:**
- Consumes: `FrameConfig`, `read_buffer`, and compiled start/end regexes.
- Produces: `drain_next_frame(buffer, frame_runtime) -> Option<Result<String, DomainError>>` supporting `line` and `block`.

- [ ] **Step 1: Write failing block-framing tests**

Add tests proving that a block can arrive in fragments, bytes before `start_regex` are discarded, the closing separator after the start ends the frame, two buffered tickets are emitted independently, and an incomplete buffer above `max_len` is discarded.

```rust
buffer.extend_from_slice(first_half);
assert!(drain_next_frame(&mut buffer, &runtime).is_none());
buffer.extend_from_slice(second_half);
assert!(drain_next_frame(&mut buffer, &runtime).unwrap().unwrap().contains("5.25pH"));
```

- [ ] **Step 2: Run block tests and verify RED**

Run: `cargo test -p infrastructure serial_ascii::tests::test_block_frame -- --nocapture`

Expected: failure because only `line` framing exists.

- [ ] **Step 3: Compile frame mode at connection initialization**

Represent line and block modes explicitly. For block mode require non-empty valid `start_regex` and `end_regex`; preserve line terminator fallback behavior. Reject unknown modes before opening the COM port.

- [ ] **Step 4: Implement block buffer draining**

Search for the start match, discard preceding bytes, then search for the end match strictly after the start match. Decode through the end match, drain consumed bytes, and retain incomplete suffixes. Apply `max_len` to incomplete blocks and log a dropped-frame warning.

- [ ] **Step 5: Route every completed frame through the selected parser**

Replace the line-only loop in `poll()` with the shared frame-draining helper. Legacy lines still call legacy mapping; version 2 blocks call configurable field mapping. Apply the same logic to mock frames.

- [ ] **Step 6: Run focused regression tests**

Run: `cargo test -p infrastructure serial_ascii -- --nocapture`

Expected: legacy lines, fallback terminators, fragmented blocks, multiple blocks, and multi-field mapping all pass.

- [ ] **Step 7: Commit block framing**

```bash
git add crates/infrastructure/src/drivers/serial_ascii.rs
git commit -m "feat(serial): add configurable block framing"
```

### Task 3: PHSJ-5 Example and End-to-End Configuration Contract

**Files:**
- Create: `crates/edge-agent/config/bootstrap.serial-phsj5.example.json`
- Modify: `crates/edge-agent/src/bootstrap.rs`
- Create: `docs/use-cases/uc-protocol-serial-phsj5.md`
- Test: `crates/edge-agent/src/bootstrap.rs` test module

**Interfaces:**
- Consumes: parser version 2, block framing, `field:*` sources, and existing `compound_json` pipeline.
- Produces: a deployable COM5/9600/8N1 example with one device and five tags (`ph`, `potential_mv`, `temperature_c`, `electrode_efficiency_pct`, `raw`).

- [ ] **Step 1: Write a failing bootstrap example test**

Parse `bootstrap.serial-phsj5.example.json`; assert one `SerialAscii` connection, block mode, parser version 2, four fields, five tags sharing `dev_phsj5_01`, `on_message` updates, and `compound_json` extraction on numeric tags.

- [ ] **Step 2: Run the bootstrap test and verify RED**

Run: `cargo test -p edge-agent test_bootstrap_phsj5_example -- --nocapture`

Expected: failure because the example file does not exist.

- [ ] **Step 3: Add the PHSJ-5 example configuration**

Use COM5, 9600/8N1, `PHSJ-5 pH Meter` as the start regex, the 16-character separator as the end regex, `max_len = 2048`, and field regexes matching captured samples from the device. Map each numeric tag to `field:<name>` and the audit tag to `raw`.

- [ ] **Step 4: Document configuration and data flow**

Document one-print/one-sample operation, compound envelope behavior, per-tag pipeline validation, repeated-value `on_message` semantics, and the fact that `Print all` is not the normal runtime mode.

- [ ] **Step 5: Run focused crate tests**

Run:

```bash
cargo test -p infrastructure serial_ascii -- --nocapture
cargo test -p application tag_pipeline -- --nocapture
cargo test -p edge-agent test_bootstrap_phsj5_example -- --nocapture
```

Expected: all commands pass with zero failures.

- [ ] **Step 6: Run formatting and compile checks**

Run:

```bash
cargo fmt --all -- --check
cargo check -p infrastructure -p application -p edge-agent
```

Expected: formatting check and compilation pass.

- [ ] **Step 7: Commit example and documentation**

```bash
git add crates/edge-agent/config/bootstrap.serial-phsj5.example.json crates/edge-agent/src/bootstrap.rs docs/use-cases/uc-protocol-serial-phsj5.md
git commit -m "docs(edge): add configurable PHSJ-5 serial example"
```

### Task 4: Final Focused Verification

**Files:**
- Verify only; no expected source changes.

**Interfaces:**
- Consumes: completed implementation and PHSJ-5 configuration.
- Produces: fresh evidence that affected crates pass while the known PostgreSQL-dependent baseline failure remains unrelated.

- [ ] **Step 1: Run affected unit suites**

```bash
cargo test -p infrastructure
cargo test -p application
cargo test -p edge-agent
```

Expected: zero failures.

- [ ] **Step 2: Inspect the final diff**

Run: `git diff --check HEAD~3..HEAD && git status --short`

Expected: no whitespace errors and a clean worktree.

- [ ] **Step 3: Summarize the known external baseline limitation**

Record that the full workspace integration suite requires reachable PostgreSQL; the pre-change failure was `api_connections_contract_tests` with Windows timeout 10060.
