# Windows Edge Safe Updater Design

**Status:** Approved for implementation on 2026-08-10.

## Goal

Update an installed Windows `edge-agent.exe` without reinstalling the edge, changing its identity/configuration, or losing queued runtime data.

## Package Contract

The release package contains:

- `bin/edge-agent.exe`
- `release-manifest.json`
- `scripts/update-edge.ps1`

The manifest declares `version`, `config_schema_version`, `minimum_central_version`, and the SHA-256 of `bin/edge-agent.exe`. The updater rejects a missing or malformed manifest, an unsupported manifest format, or a hash mismatch before stopping the runtime.

## Transaction

1. Resolve and validate package and installation paths.
2. Validate the manifest and binary SHA-256.
3. Resolve exactly one NSSM/Windows service by literal name or one scheduled task by literal name and task path, unless the mode is explicit.
4. Stop only that named runtime and any `edge-agent.exe` whose executable path is the installation target.
5. Copy the installed binary and manifest into a unique `InstallRoot\releases\<old-version>\<snapshot>` directory and verify the snapshot hashes.
6. Stage the incoming binary beside the target and atomically replace the target.
7. Restart the same service or task without recreating it.
8. Wait a bounded interval for an `edge-agent.exe` process with the exact target path.
9. On failure, stop the named runtime, restore the backup, restart it, and return a failing exit code.

## Preservation and Safety

- The updater never writes under `DataRoot`; it therefore preserves `edge.env`, bootstrap configuration, signed runtime cache, MQTT outbox, receipts, and logs.
- It does not recreate task credentials or service definitions.
- Runtime names cannot contain wildcard characters, task lookup includes the exact task folder, and cmdlets receive the resolved service/task object rather than a name pattern.
- It never stops all processes named `edge-agent`; process fallback is restricted to an exact executable path.
- Package, install, and data roots cannot overlap or traverse reparse points.
- Backup directories are versioned and retained for manual recovery.
- A successful update records the installed release in `InstallRoot\release-manifest.json`.

## Verification

PowerShell contract tests run against isolated temporary directories with strongly gated test hooks. They cover hash and manifest rejection before mutation, unique snapshots, successful update while preserving `DataRoot`, automatic rollback, restart after pre-replacement failure, root overlap, and exact scheduled-task selection through Windows cmdlet mocks. A release build supplies the binary and a generated manifest supplies its real SHA-256.
