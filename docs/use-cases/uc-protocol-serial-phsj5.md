# UC-PROTOCOL-005: INE PHSJ-5 configurable multi-field input

## Goal

Read one immediate `PRINT` transmission from an INE PHSJ-5 over USB serial and publish all measurements as tags belonging to the same device.

The runtime contract is intentionally one immediate print at a time; the instrument's `Print all`/saved-record dump is not used for normal acquisition.

Captured device example at 9600 8N1:

```text
93.79mV
5.25pH
24.2c
98.82%
```

## Configuration

Use `crates/edge-agent/config/bootstrap.serial-phsj5.example.json` as a deployment example and select it with `EDGE_BOOTSTRAP_PATH`. Its `serial.port`, connection/tag IDs and `device_id` values are ordinary configuration: `COM5` and the `phsj5`-based identifiers are illustrative and can be replaced without code changes.

The connection uses:

- `frame.mode = block` to assemble one complete print, even when serial reads arrive fragmented.
- `frame.start_regex` and `frame.end_regex` to delimit the print.
- `parser.version = 2` and named `parser.fields`.
- `tag_map` sources in the form `field:<name>` plus the reserved `raw` source.

Each named field has an independent regular expression, capture groups, value type and `required` policy. `unit_group` is captured from the incoming data; it is not imposed by the pipeline.

## Output contract

Each `field:<name>` produces the existing compound JSON envelope:

```json
{"value":5.25,"unit":"pH","raw":"5.25pH"}
```

The configured per-tag pipeline uses `extract: compound_json` to expose the typed value while preserving unit and raw context. No new pipeline or multi-tag abstraction is required.

The `raw` source publishes the complete four-line print. All five tags share the configured device ID and use `update_mode = on_message`, so repeated valid readings are not suppressed.

## Compatibility

Parser version 1 remains the default. Existing scale configurations (`scale:compound`, `scale:value`, `scale:unit`, and `raw`) continue to use line framing and the legacy parser unchanged.

Parser version 2 validates regexes, capture groups, value types and `field:<name>` mappings during driver initialization. A missing required field yields an error only for its mapped tag; valid sibling fields are still emitted. A missing optional field emits no update.
