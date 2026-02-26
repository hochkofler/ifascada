# UC-RUNTIME-021: Generic Automation Engine (Immutable Domain)

## Objective
Move automation logic to a generic, reusable model without coupling domain to infrastructure, while keeping current action command flow.

## Final Design
1. `domain` (pure, immutable):
   - `AutomationSpec`, `TriggerSpec`, `ActionSpec`, `ExecutionScope`
   - supports backward-compatible `action` and multi-action `actions[]`
   - `AutomationEvalState` + `evaluate_automation(...)`
2. `application` (stateful orchestration):
   - `AutomationEngine` keeps trigger state and emits `ActionRequest`
3. `edge-agent` (infrastructure/runtime):
   - consumes `ActionRequest`
   - executes via existing action executor (`print.escpos`)
   - publishes result/audit in the same MQTT contract

## Config Source of Truth
1. Primary:
   - Central signed runtime config (DB catalog) now includes:
   - `automations` (top-level array in payload)
   - supports both `connections.metadata_json.automations[]` and `tags.metadata_json.automations[]`
   - for tag-scoped automations, `trigger.tag_id` is auto-inferred from `tag_code` when omitted
2. Fallback:
   - local bootstrap can include `automations`
3. Legacy compatibility:
   - env-based auto-print is converted internally to automation specs (fallback only)

## Runtime Payload Example
```json
{
  "connections": [ ... ],
  "automations": [
    {
      "id": "auto-scale-non-positive",
      "name": "print_on_non_positive",
      "enabled": true,
      "trigger": {
        "type": "consecutive_numeric",
        "tag_id": "tag_scale_manual_compound",
        "threshold": 0.0,
        "count": 2,
        "operator": "lte",
        "within_ms": 5000
      },
      "actions": [
        {
          "action_type": "print.escpos",
          "target": "edge",
          "scope": "edge",
          "payload": {
            "lines": ["AUTO TRIGGER", "Scale alarm"]
          }
        },
        {
          "action_type": "print.persist",
          "target": "central",
          "scope": "central",
          "payload": {
            "mode": "audit_only"
          }
        }
      ]
    }
  ]
}
```

Notes:
1. `action` (single) is still accepted for legacy compatibility.
2. Prefer `actions[]` when a trigger must drive both local (`edge`) and central (`central`) behavior.

## Scope Rules
1. `scope=edge`: execute only on edge runtime.
2. `scope=central`: central intent; current edge runtime can forward/execute central-intent actions for durability workflows (e.g. `print.persist`).
3. `scope=auto`: accepted by current runtime; use explicit scope in production to avoid ambiguity.

## TDD Coverage Added
1. Domain:
   - consecutive trigger evaluation
   - reset behavior when condition breaks
   - numeric extraction from JSON string payload
2. Application:
   - action emission after required consecutive matches
   - scope filtering (`edge` vs `central`) including edge-forward for central-intent actions
   - multi-action emission from one trigger (`actions[]`)
3. Edge bootstrap:
   - valid automations parsed
   - invalid entries ignored with warning
