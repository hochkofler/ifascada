# SerialAscii Configurable Multi-Field Parser Design

## Goal

Generalize the existing `SerialAscii` connection so one configured frame can produce multiple tag values without adding device-specific Rust drivers. The first real configuration targets the INE-PHSJ-5 pH meter, while legacy scale configurations remain unchanged.

## Existing Capabilities Reused

- `DriverConnection::poll()` already returns multiple `(TagId, TagValue)` results.
- `tag_map` already binds connection outputs to tags belonging to a device.
- Each tag already has an independent metadata pipeline for compound extraction, scaling, range validation, formatting, and trigger selection.
- Central and edge bootstrap already transport connection-level `frame`, `parser`, and tag sources.

## Configuration Contract

Legacy configuration remains parser version 1 by default and continues to accept one line containing sign, value, and unit.

Parser version 2 adds configurable fields:

```json
{
  "frame": {
    "mode": "block",
    "start_regex": "PHSJ-5 pH Meter",
    "end_regex": "(?m)^={16}\\s*$",
    "max_len": 2048
  },
  "parser": {
    "version": 2,
    "fields": {
      "ph": {
        "regex": "(?m)^\\s*([-+]?\\d+(?:\\.\\d+)?)\\s*(pH)\\s*$",
        "value_group": 1,
        "unit_group": 2,
        "value_type": "float",
        "required": true
      }
    }
  },
  "tag_map": {
    "tag_ph": "field:ph",
    "tag_raw": "raw"
  }
}
```

Each field regex is evaluated against the complete normalized frame. `value_type` accepts `float` (default), `integer`, `string`, or `boolean`; `unit_group` is optional. A field produces the existing compound JSON envelope:

```json
{"value":5.25,"unit":"pH","raw":"5.25pH"}
```

The mapped tag's existing `compound_json` pipeline then extracts `value`, preserves `unit` and `raw` as context, and applies validation or formatting. The parser does not hard-code field names or units.

## Framing

- `line` remains the default and preserves the scale behavior.
- `block` discards bytes before the configured start match, then emits through the first end match that occurs after the start.
- Control characters may remain in the serial buffer; field regexes operate on the decoded block and can ignore them.
- Oversized incomplete blocks are discarded at `max_len` to prevent unbounded buffering.
- A block may arrive across multiple serial reads and multiple blocks may be decoded from one read buffer.

## Mapping and Errors

- Legacy sources remain: `scale:compound`, `scale:value`, `scale:unit`, and `scale:raw`, including their aliases.
- Version 2 adds `field:<name>` and `raw` sources.
- Unknown mapped fields, unsupported value types, invalid capture groups, or invalid regular expressions fail connection initialization.
- A captured value that cannot be converted to its configured type returns a tag-level parse error.
- A missing required field returns a tag-level parse error for its mapped tag.
- A missing optional field produces no update for that tag.
- Successfully parsed sibling fields are still emitted when another field is absent or invalid.
- The reserved `raw` output contains the complete frame.

## PHSJ-5 Outputs

The example connection defines four fields and tags:

- `ph`
- `potential_mv`
- `temperature_c`
- `electrode_efficiency_pct`

One operator `PRINT` action yields one block and one poll result containing all successfully parsed tags. `SAVE` and `Print all` are outside the normal runtime flow.

## Scope

Implementation is localized to `SerialAscii` framing, parser configuration, output routing, tests, and an example bootstrap. The runtime multi-tag contract and tag pipeline require no behavioral changes.

## Verification

- Existing scale parser and output tests remain green.
- New unit tests cover configuration validation, fragmented block assembly, real PHSJ-5 parsing, multiple tags, raw output, optional fields, required-field errors, and multiple buffered blocks.
- Focused tests run for the `infrastructure` and `application` crates.
