import { describe, it, expect } from "vitest";
import { findPrintDeviceCommand, findPrintPersistAction } from "./print-metadata";

describe("findPrintDeviceCommand", () => {
  it("finds a device.command 'print' automation among a tag's metadata_json.automations", () => {
    const meta = {
      automations: [
        {
          enabled: true,
          actions: [{ action_type: "device.command", payload: { command: "print", args: { foo: 1 } } }],
        },
      ],
    };
    expect(findPrintDeviceCommand(meta)).toEqual({ command: "print", args: { foo: 1 } });
  });

  it("also matches command 'print.escpos'", () => {
    const meta = {
      automations: [
        { enabled: true, action: { action_type: "device.command", payload: { command: "print.escpos" } } },
      ],
    };
    expect(findPrintDeviceCommand(meta)).toEqual({ command: "print.escpos" });
  });

  it("ignores a disabled automation", () => {
    const meta = {
      automations: [
        {
          enabled: false,
          actions: [{ action_type: "device.command", payload: { command: "print" } }],
        },
      ],
    };
    expect(findPrintDeviceCommand(meta)).toBeNull();
  });

  it("ignores device.command automations whose command isn't a print variant", () => {
    const meta = {
      automations: [{ enabled: true, actions: [{ action_type: "device.command", payload: { command: "reboot" } }] }],
    };
    expect(findPrintDeviceCommand(meta)).toBeNull();
  });

  it("returns null when metadata is undefined or has no automations", () => {
    expect(findPrintDeviceCommand(undefined)).toBeNull();
    expect(findPrintDeviceCommand({})).toBeNull();
  });
});

describe("findPrintPersistAction", () => {
  it("finds a print.persist automation", () => {
    const meta = {
      automations: [{ enabled: true, actions: [{ action_type: "print.persist", payload: { retain_days: 30 } }] }],
    };
    expect(findPrintPersistAction(meta)).toEqual({
      action_type: "print.persist",
      payload: { retain_days: 30 },
    });
  });

  it("returns null when there is no print.persist automation", () => {
    const meta = {
      automations: [{ enabled: true, actions: [{ action_type: "device.command", payload: { command: "print" } }] }],
    };
    expect(findPrintPersistAction(meta)).toBeNull();
  });
});
