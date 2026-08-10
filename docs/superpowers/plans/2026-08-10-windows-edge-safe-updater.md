# Windows Edge Safe Updater Implementation Plan

**Goal:** Ship a transactional Windows binary updater that preserves all edge configuration/data and rolls back on failed startup.

**Architecture:** Keep installation and configuration workflows unchanged. Add a package-local updater driven by a release manifest. Runtime control is restricted to the configured service/task and exact installed executable path; file replacement uses a versioned backup and rollback transaction.

**Tech Stack:** Windows PowerShell 5.1, Pester 3, Cargo release build.

## Task 1: Executable Contract Tests

**Files:**

- Add: `deploy/edge-1.0.0-runtime/tests/update-edge.Tests.ps1`

1. Create isolated package, installation, and data directories per test.
2. Write a failing test proving a hash mismatch leaves the installed binary untouched.
3. Write a failing successful-update test proving backup creation, binary replacement, installed-manifest update, and byte-for-byte preservation of `DataRoot`.
4. Write a failing health-check test proving the old binary is restored.
5. Run Pester and capture the expected RED result.

## Task 2: Transactional Updater

**Files:**

- Add: `deploy/edge-1.0.0-runtime/scripts/update-edge.ps1`

1. Validate resolved paths, manifest fields, and binary SHA-256 before runtime mutation.
2. Implement named service/task detection and exact-path process handling.
3. Implement unique versioned snapshots, staged replacement, exact runtime-object restart, and bounded health detection.
4. Implement rollback and restart on any post-stop failure.
5. Add strongly gated test hooks plus Windows cmdlet mocks so filesystem tests need no administrator rights while exact task selection is still exercised.
6. Run Pester until all updater tests pass.

## Task 3: Release Package and Documentation

**Files:**

- Add: `deploy/edge-1.0.0-runtime/release-manifest.example.json`
- Add: `deploy/edge-1.0.0-runtime/scripts/build-edge-package.ps1`
- Modify: `deploy/edge-1.0.0-runtime/README.md`

1. Document the update command, preservation guarantees, validation, backups, and rollback behavior.
2. Add a reproducible package builder that compiles `edge-agent.exe`, copies it into the package, and generates the ignored release manifest with its real SHA-256.
3. Build the release package locally at version `1.1.0`.
4. Re-run Pester against the final package.

## Task 4: Final Verification and Publication

1. Confirm the worktree contains only intended files.
2. Run focused Rust tests for the edge agent and SerialAscii parser.
3. Run the complete updater Pester suite from a clean temporary state.
4. Commit, push, and open a PR for review.
