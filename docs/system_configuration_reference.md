# System Configuration Reference

This document provides a comprehensive reference for all configurable components in the IFASCADA system, including Drivers, Pipelines, and Automations.

## 1. Drivers

The `driver` field in the tag configuration determines how the system connects to the device. The `driver_config` field must match the selected driver type.

### 1.1 RS232 (Serial)
Used for serial communication (e.g., scales, sensors).

**Configuration Schema:**
```json
{
  "port": "COM1",           // Required: Serial port name (e.g., COM1, /dev/ttyUSB0)
  "baud_rate": 9600,        // Default: 9600
  "data_bits": 8,           // Default: 8 (Options: 5, 6, 7, 8)
  "parity": "None",         // Default: "None" (Options: "None", "Even", "Odd")
  "stop_bits": 1,           // Default: 1 (Options: 1, 2)
  "timeout_ms": 1000        // Default: 1000. Read timeout in milliseconds.
}
```

### 1.2 Simulator
Used for testing and development. Generates values based on a pattern.

**Configuration Schema:**
```json
{
  "min_value": 0.0,         // Minimum generated value
  "max_value": 100.0,       // Maximum generated value
  "interval_ms": 1000,      // Time between updates in milliseconds
  "unit": "kg",             // Unit string appended to value
  "pattern": "sine"         // Optional: Generation pattern (currently only "sine" implemented)
}
```

### 1.3 Modbus, OPC-UA, HTTP
*Not yet implemented.*

---

## 2. Pipelines

The `pipeline` field allows transforming and validating data before it is processed. It consists of an optional `parser` and a list of `validators`.

### 2.1 Parsers (`parser`)
Extracts a structured value from the raw driver output.

**Type: `Regex`**
Extracts the first capture group from a regex pattern.
```json
{
  "type": "Regex",
  "pattern": "GROSS WEIGHT:\\s*([0-9.]+)"
}
```

**Type: `Json`**
Extracts a field from a JSON object using a path.
```json
{
  "type": "Json",
  "path": "weight.current"
}
```

**Type: `None`**
Passes the raw value through as-is.

### 2.2 Validators (`validators`)
Checks if the parsed value is valid.

**Type: `Range`**
Checks if a numeric value is within a specific range.
```json
{
  "type": "Range",
  "min": 10.0,              // Optional
  "max": 50.0               // Optional
}
```

**Type: `Contains`**
Checks if the string representation contains a substring.
```json
{
  "type": "Contains",
  "substring": "STABLE"
}
```

---

## 3. Automations

The `automations` list allows defining rules to trigger actions based on tag values.

### 3.1 Triggers (`trigger`)
Defines WHEN an action should fire.

**Type: `ConsecutiveValues`**
Fires when a value matches a condition `count` times in a row.
```json
{
  "type": "ConsecutiveValues",
  "target_value": 0.0,      // The value to check against
  "count": 2,               // Number of consecutive matches required
  "operator": "Equal",      // "Equal", "LessOrEqual", "GreaterOrEqual" (Default: "Equal")
  "within_ms": 5000         // Optional: Reset count if no events within this window
}
```

### 3.2 Actions (`action`)
Defines WHAT happens when a trigger fires.

**Type: `PrintTicket`**
Sends a print command (currently mocks the output).
```json
{
  "type": "PrintTicket",
  "template": "WEIGHT_TICKET", // Name of the template to use
  "service_url": "http://..."  // Optional: External print service URL
}
```

**Type: `PublishMqtt`**
Publishes a message to a specific MQTT topic.
```json
{
  "type": "PublishMqtt",
  "topic": "alerts/scale/zero",
  "payload_template": "Scale {{tag_id}} is at zero."
}
```
