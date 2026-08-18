# CI/CD Pipeline for central-server and web-ui — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `central-server` and `web-ui` a tag-triggered, tested, gated, auto-rollback-on-failure
build-and-deploy pipeline via self-hosted GitHub Actions, closing the gap with `edge-agent`'s
already-mature manual updater.

**Architecture:** A tag push (`central-vX.Y.Z` / `webui-vX.Y.Z`) triggers a per-component GitHub
Actions workflow on a self-hosted runner (the developer's own workstation, `.193`). The workflow
builds a Docker image, publishes it as a GitHub Release artifact, waits for a manual approval
(`production` environment), then deploys by copying the image to the target host over SSH (a
dedicated, narrowly-scoped deploy key), swapping the running container, and polling its health
endpoint — rolling back automatically to the previous image on failure.

**Tech Stack:** GitHub Actions (self-hosted runner), Docker, PowerShell 5.1 + Pester (matching
`deploy/edge-1.0.0-runtime`'s existing test convention), Next.js App Router (`web-ui`), Rust/Cargo
(`central-server`).

**Spec:** `docs/superpowers/specs/2026-08-14-cicd-deployment-protocol-design.md` (amended
2026-08-18) — this plan implements everything in that spec **except** the "Edge-Agent Update
Protocol" section, which is a separate plan (edges pull their own updates; nothing here touches
an edge machine).

## Global Constraints

- Per-component workflows trigger **only** on their own tag prefix (`central-v*` for this plan's
  central-server workflow, `webui-v*` for web-ui) — a tag for one component must never trigger a
  build of the other.
- The self-hosted runner lives on the user's own workstation (IP `192.168.103.193` throughout this
  plan), not on `.154` — `.154` is the constrained 8GB/4-core production host and must not compete
  with CI builds.
- The CI deploy SSH key is **dedicated and newly generated for this pipeline** — never the personal
  keys (`ifascada_migracion_154`, `id_ed25519`) used for today's manual migration work — and is
  scoped to `central-server`/`web-ui` hosts only (today: `.154`, user `ifa`). It must never be
  authorized on any edge machine.
- Update Protocol v1 has **no zero-downtime guarantee** — a short stop-old/start-new cutover window
  is accepted. Health-check timeout and rollback are mandatory; a silent unhealthy deploy is not
  acceptable.
- `POSTGRES_HOST_AUTH_METHOD=trust` stays as-is — hardening it is explicitly out of scope for this
  plan (tracked as a spec follow-up).
- Every new PowerShell function must be independently testable with Pester without requiring a real
  network connection or a real Docker daemon — network/Docker calls are isolated behind named
  functions (`Invoke-RemoteCommand`, `Copy-ToRemote`) that tests mock.

---

## File Structure

**New files this plan creates:**

- `central-server/Dockerfile` — multi-stage Rust build (reconstructed from the currently-deployed
  image's `docker history`; never committed to this repo before — see Task 1).
- `web-ui/Dockerfile` — multi-stage Next.js build (same situation — see Task 2).
- `web-ui/app/api/health/route.ts` — new health-check endpoint.
- `scripts/lib/DeployDockerService.ps1` — all deploy-protocol functions (pure helpers + I/O seams +
  orchestrator), dot-sourced by both the CLI entrypoint and its Pester tests.
- `scripts/deploy-docker-service.ps1` — thin CLI entrypoint over the library above.
- `scripts/tests/DeployDockerService.Tests.ps1` — Pester tests for the library.
- `.github/workflows/central-server.yml`
- `.github/workflows/web-ui.yml`
- `.github/workflows/edge-agent.yml`

No existing files are modified by this plan.

---

### Task 1: `central-server` Dockerfile

**Context:** No Dockerfile for `central-server` exists anywhere in this repository's history — the
images currently deployed (`ifascada/central-server:1.0.0` through `1.0.2`) were built by an
undocumented manual process. Reconstructed from `docker history --no-trunc
ifascada/central-server:1.0.2` run against the image currently loaded on `.154`, which showed the
runtime stage was `debian:bookworm` with `ca-certificates` installed, the binary copied from
`/app/target/release/central-server`, and `crates/edge-agent/config` copied to `/app/config`. The
builder stage's own layers aren't visible in `docker history` (multi-stage builds discard
intermediate stages), so the builder half below is written fresh against the real workspace
structure (`crates/central-server` is a member of the root `Cargo.toml` workspace).

**Files:**
- Create: `central-server/Dockerfile`

**Interfaces:**
- Produces: a `docker build -f central-server/Dockerfile -t ifascada/central-server:<version> .`
  invocation, run from the repo root, that Task 12's workflow depends on.

- [ ] **Step 1: Write the Dockerfile**

```dockerfile
# central-server/Dockerfile
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --package central-server

FROM debian:bookworm
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/central-server /usr/local/bin/central-server
COPY crates/edge-agent/config /app/config
ENTRYPOINT ["/usr/local/bin/central-server"]
```

- [ ] **Step 2: Build it locally and compare against the currently-deployed image**

Run from the repo root:
```powershell
docker build -f central-server/Dockerfile -t ifascada/central-server:local-test .
docker history --no-trunc ifascada/central-server:local-test
```
Expected: the final two layers (`COPY .../central-server`, `COPY crates/edge-agent/config`,
`ENTRYPOINT`) match the structure seen in the 1.0.2 image's history (same COPY sources/destinations
listed in the Context note above). Layer sizes will differ slightly (compiler/dependency version
drift over time is expected and fine) — what must match is the **shape**: same base image family,
same binary path, same entrypoint.

- [ ] **Step 3: Run it and confirm it starts the same way as the deployed container**

```powershell
docker run --rm -e CENTRAL_MQTT_ENABLED=false -e CENTRAL_API_ENABLED=true -p 18088:8088 ifascada/central-server:local-test
```
Expected: log line `central-server API listening on 0.0.0.0:8088` (same line format seen in
today's production logs) within a few seconds, no immediate crash. Stop with Ctrl+C.

- [ ] **Step 4: Commit**

```bash
git add central-server/Dockerfile
git commit -m "build(central-server): add Dockerfile, reconstructed from deployed image history

No Dockerfile for central-server was ever committed to this repo; the
images running in production today were built by an undocumented
manual process. Reconstructed via 'docker history' against the
currently-deployed 1.0.2 image so the CI pipeline has a real, verified
build definition to work from."
```

---

### Task 2: `web-ui` Dockerfile

**Context:** Same situation as Task 1 — reconstructed from `docker history --no-trunc
ifascada/web-ui:1.0.2`, which showed a `node:20-alpine`-based runtime stage with `NODE_ENV=production`,
`NEXT_TELEMETRY_DISABLED=1`, `PORT=3001`, `npm ci --omit=dev` run in the runtime stage itself (not a
separate deps stage), and `.next` + `next.config.mjs` copied in from a discarded builder stage —
consistent with `web-ui/package.json`'s existing `"start": "next start -p 3001"` script (confirmed
today) and no `output: 'standalone'` in the Next.js config (a standalone build wouldn't need its own
`npm ci` in the runtime stage).

**Files:**
- Create: `web-ui/Dockerfile`

**Interfaces:**
- Produces: a `docker build -f web-ui/Dockerfile -t ifascada/web-ui:<version> .` invocation, run
  from the repo root, that Task 13's workflow depends on.

- [ ] **Step 1: Write the Dockerfile**

```dockerfile
# web-ui/Dockerfile
FROM node:20-alpine AS builder
WORKDIR /app
COPY web-ui/package.json web-ui/package-lock.json ./
RUN npm ci
COPY web-ui/ .
RUN npm run build

FROM node:20-alpine
WORKDIR /app
ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
ENV PORT=3001
COPY web-ui/package.json web-ui/package-lock.json ./
RUN npm ci --omit=dev
COPY --from=builder /app/.next ./.next
COPY --from=builder /app/next.config.mjs ./next.config.mjs
EXPOSE 3001
CMD ["npm", "run", "start", "--", "-p", "3001"]
```

- [ ] **Step 2: Build it locally and compare against the currently-deployed image**

```powershell
docker build -f web-ui/Dockerfile -t ifascada/web-ui:local-test .
docker history --no-trunc ifascada/web-ui:local-test
```
Expected: same layer shape as the 1.0.2 image (COPY package.json/package-lock, `npm ci --omit=dev`,
COPY `.next`, COPY `next.config.mjs`, same `ENV`/`EXPOSE`/`CMD`).

- [ ] **Step 3: Run it and confirm the app serves**

```powershell
docker run --rm -p 13001:3001 -e CENTRAL_API_UPSTREAM=http://127.0.0.1:8088 ifascada/web-ui:local-test
```
In another terminal: `Invoke-WebRequest http://127.0.0.1:13001 -UseBasicParsing` — expect a `200`
status with HTML content. Stop with Ctrl+C.

- [ ] **Step 4: Commit**

```bash
git add web-ui/Dockerfile
git commit -m "build(web-ui): add Dockerfile, reconstructed from deployed image history

Same situation as central-server: no Dockerfile was ever committed.
Reconstructed via 'docker history' against the currently-deployed 1.0.2
image."
```

---

### Task 3: `/api/health` endpoint in web-ui

**Files:**
- Create: `web-ui/app/api/health/route.ts`

**Interfaces:**
- Produces: `GET /api/health` -> `200 {"status":"ok"}`, consumed by Task 7's `Test-ServiceHealthy`
  during deploys and by Task 13's workflow.

**Note on testing:** `web-ui` has no test runner configured today (`package.json` scripts are only
`dev`/`build`/`start`/`lint` — confirmed by inspection; no jest/vitest present). Introducing a whole
test framework for one route would violate the "follow existing patterns" guidance more than it
would help, so this task's verification step is `npm run build` (the same gate the spec's Testing
section already specifies for `web-ui`) plus a manual runtime check, matching how the rest of
`web-ui` is verified today.

- [ ] **Step 1: Write the endpoint**

```typescript
// web-ui/app/api/health/route.ts
import { NextResponse } from "next/server";

export async function GET() {
  return NextResponse.json({ status: "ok" });
}
```

- [ ] **Step 2: Verify it builds**

```powershell
cd web-ui
npm run build
```
Expected: build succeeds, and the build output lists `/api/health` as a route (Next.js prints a
route table on successful build).

- [ ] **Step 3: Verify it responds correctly at runtime**

```powershell
npm run dev
```
In another terminal: `Invoke-WebRequest http://127.0.0.1:3001/api/health -UseBasicParsing` —
expect status `200` and body `{"status":"ok"}`. Stop the dev server with Ctrl+C.

- [ ] **Step 4: Commit**

```bash
git add web-ui/app/api/health/route.ts
git commit -m "feat(web-ui): add /api/health endpoint

Required by the CI/CD deploy protocol to poll web-ui's health after a
deploy, the same way central-server already exposes /health/live."
```

---

### Task 4: `.env` parsing/editing helpers

**Files:**
- Create: `scripts/lib/DeployDockerService.ps1`
- Test: `scripts/tests/DeployDockerService.Tests.ps1`

**Interfaces:**
- Produces: `Get-CurrentImageTag(-EnvContent [string], -VarName [string]) -> [string]` and
  `Set-ImageTag(-EnvContent [string], -VarName [string], -NewValue [string]) -> [string]`. Both are
  pure string functions — no file or network I/O — consumed by Task 7's orchestrator.

- [ ] **Step 1: Write the failing tests**

```powershell
# scripts/tests/DeployDockerService.Tests.ps1
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$libPath = Join-Path (Split-Path -Parent $here) "lib\DeployDockerService.ps1"
. $libPath

Describe "Get-CurrentImageTag" {
    It "extracts the value of the named variable" {
        $env = "RUST_LOG=info`r`nCENTRAL_IMAGE=ifascada/central-server:1.0.2`r`nCENTRAL_API_PORT=8088"
        Get-CurrentImageTag -EnvContent $env -VarName "CENTRAL_IMAGE" | Should Be "ifascada/central-server:1.0.2"
    }

    It "throws when the variable is not present" {
        $env = "RUST_LOG=info"
        { Get-CurrentImageTag -EnvContent $env -VarName "CENTRAL_IMAGE" } | Should Throw
    }
}

Describe "Set-ImageTag" {
    It "replaces the value of the named variable, leaving other lines untouched" {
        $env = "RUST_LOG=info`r`nCENTRAL_IMAGE=ifascada/central-server:1.0.2`r`nCENTRAL_API_PORT=8088"
        $result = Set-ImageTag -EnvContent $env -VarName "CENTRAL_IMAGE" -NewValue "ifascada/central-server:1.0.3"
        $result | Should Match "CENTRAL_IMAGE=ifascada/central-server:1.0.3"
        $result | Should Match "RUST_LOG=info"
        $result | Should Match "CENTRAL_API_PORT=8088"
    }

    It "throws instead of appending when the variable is not present" {
        $env = "RUST_LOG=info"
        { Set-ImageTag -EnvContent $env -VarName "CENTRAL_IMAGE" -NewValue "x" } | Should Throw
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `Invoke-Pester scripts/tests/DeployDockerService.Tests.ps1`
Expected: FAIL — `DeployDockerService.ps1` (the library file) does not exist yet, so dot-sourcing it
errors out before any `Describe` block runs.

- [ ] **Step 3: Write the minimal implementation**

```powershell
# scripts/lib/DeployDockerService.ps1

function Get-CurrentImageTag {
    param(
        [Parameter(Mandatory)][string]$EnvContent,
        [Parameter(Mandatory)][string]$VarName
    )
    $pattern = "(?m)^$([regex]::Escape($VarName))=(.*)$"
    $match = [regex]::Match($EnvContent, $pattern)
    if (-not $match.Success) {
        throw "Variable '$VarName' not found in env content"
    }
    return $match.Groups[1].Value.Trim()
}

function Set-ImageTag {
    param(
        [Parameter(Mandatory)][string]$EnvContent,
        [Parameter(Mandatory)][string]$VarName,
        [Parameter(Mandatory)][string]$NewValue
    )
    $lines = $EnvContent -split "`r?`n"
    $found = $false
    $result = for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match "^$([regex]::Escape($VarName))=") {
            $found = $true
            "$VarName=$NewValue"
        } else {
            $lines[$i]
        }
    }
    if (-not $found) {
        throw "Variable '$VarName' not found in env content; refusing to append a new one"
    }
    return ($result -join "`r`n")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `Invoke-Pester scripts/tests/DeployDockerService.Tests.ps1`
Expected: PASS, all 4 tests green.

- [ ] **Step 5: Commit**

```bash
git add scripts/lib/DeployDockerService.ps1 scripts/tests/DeployDockerService.Tests.ps1
git commit -m "feat(deploy): add pure .env image-tag read/write helpers

First pieces of the central-server/web-ui deploy protocol library.
Kept as pure string functions (no file or network I/O) so they're
trivially unit-testable; the orchestrator in a later task supplies the
actual .env content read from the remote host."
```

---

### Task 5: Health-poll function

**Files:**
- Modify: `scripts/lib/DeployDockerService.ps1`
- Modify: `scripts/tests/DeployDockerService.Tests.ps1`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `Test-ServiceHealthy(-Url [string], -MaxAttempts [int] = 30, -PollIntervalSeconds [int]
  = 2) -> [bool]`, consumed by Task 7's orchestrator.

**Design note:** built around attempt-count, not wall-clock time, specifically so it's testable
without real waiting — `Start-Sleep` gets mocked to a no-op in tests, and attempt count is
deterministic (unlike a real timer, which would make "never becomes healthy" tests either slow or
flaky).

- [ ] **Step 1: Write the failing tests**

```powershell
# append to scripts/tests/DeployDockerService.Tests.ps1

Describe "Test-ServiceHealthy" {
    BeforeEach {
        Mock Start-Sleep {}
    }

    It "returns true immediately when the first check succeeds" {
        Mock Invoke-WebRequest { [pscustomobject]@{ StatusCode = 200 } }
        Test-ServiceHealthy -Url "http://example.invalid/health" -MaxAttempts 5 -PollIntervalSeconds 1 | Should Be $true
        Assert-MockCalled Invoke-WebRequest -Times 1 -Exactly
    }

    It "retries after a failed attempt and succeeds on the next one" {
        $script:callCount = 0
        Mock Invoke-WebRequest {
            $script:callCount++
            if ($script:callCount -lt 3) { throw "connection refused" }
            [pscustomobject]@{ StatusCode = 200 }
        }
        Test-ServiceHealthy -Url "http://example.invalid/health" -MaxAttempts 5 -PollIntervalSeconds 1 | Should Be $true
        $script:callCount | Should Be 3
    }

    It "returns false after exhausting all attempts" {
        Mock Invoke-WebRequest { throw "connection refused" }
        Test-ServiceHealthy -Url "http://example.invalid/health" -MaxAttempts 3 -PollIntervalSeconds 1 | Should Be $false
        Assert-MockCalled Invoke-WebRequest -Times 3 -Exactly
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `Invoke-Pester scripts/tests/DeployDockerService.Tests.ps1`
Expected: FAIL — `Test-ServiceHealthy` is not defined.

- [ ] **Step 3: Write the minimal implementation**

```powershell
# append to scripts/lib/DeployDockerService.ps1

function Test-ServiceHealthy {
    param(
        [Parameter(Mandatory)][string]$Url,
        [int]$MaxAttempts = 30,
        [int]$PollIntervalSeconds = 2
    )
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        try {
            $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 5
            if ($response.StatusCode -eq 200) {
                return $true
            }
        } catch {
            # not up yet, keep polling
        }
        if ($attempt -lt $MaxAttempts) {
            Start-Sleep -Seconds $PollIntervalSeconds
        }
    }
    return $false
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `Invoke-Pester scripts/tests/DeployDockerService.Tests.ps1`
Expected: PASS, all 7 tests green (4 from Task 4 + 3 new).

- [ ] **Step 5: Commit**

```bash
git add scripts/lib/DeployDockerService.ps1 scripts/tests/DeployDockerService.Tests.ps1
git commit -m "feat(deploy): add attempt-count-based health poll function

Attempt-count rather than wall-clock time, specifically so 'never
becomes healthy' can be tested deterministically and fast (Start-Sleep
mocked to a no-op) instead of needing a real timer in tests."
```

---

### Task 6: Remote command/copy transport functions

**Files:**
- Modify: `scripts/lib/DeployDockerService.ps1`
- Modify: `scripts/tests/DeployDockerService.Tests.ps1`

**Interfaces:**
- Produces: `Invoke-RemoteCommand(-TargetHost, -SshUser, -SshKeyPath, -Command [string]) -> [string]`
  (returns captured stdout, throws on nonzero exit) and `Copy-ToRemote(-TargetHost, -SshUser,
  -SshKeyPath, -LocalPath, -RemotePath)` (throws on failure). Consumed by Task 7's orchestrator.
  These are the only two functions in the library that touch the network — every other function is
  pure or mockable through these two.

- [ ] **Step 1: Write the failing tests**

```powershell
# append to scripts/tests/DeployDockerService.Tests.ps1

Describe "Invoke-RemoteCommand" {
    It "returns captured output on success" {
        Mock ssh { "remote output line" ; $global:LASTEXITCODE = 0 }
        $result = Invoke-RemoteCommand -TargetHost "192.168.103.154" -SshUser "ifa" -SshKeyPath "C:\key" -Command "echo hi"
        $result | Should Be "remote output line"
    }

    It "throws when the remote command fails" {
        Mock ssh { "some error"; $global:LASTEXITCODE = 1 }
        { Invoke-RemoteCommand -TargetHost "192.168.103.154" -SshUser "ifa" -SshKeyPath "C:\key" -Command "false" } | Should Throw
    }
}

Describe "Copy-ToRemote" {
    It "does not throw on a successful copy" {
        Mock scp { $global:LASTEXITCODE = 0 }
        { Copy-ToRemote -TargetHost "192.168.103.154" -SshUser "ifa" -SshKeyPath "C:\key" -LocalPath "C:\local.tar" -RemotePath "C:/remote.tar" } | Should Not Throw
    }

    It "throws when the copy fails" {
        Mock scp { $global:LASTEXITCODE = 1 }
        { Copy-ToRemote -TargetHost "192.168.103.154" -SshUser "ifa" -SshKeyPath "C:\key" -LocalPath "C:\local.tar" -RemotePath "C:/remote.tar" } | Should Throw
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `Invoke-Pester scripts/tests/DeployDockerService.Tests.ps1`
Expected: FAIL — `Invoke-RemoteCommand` and `Copy-ToRemote` are not defined.

- [ ] **Step 3: Write the minimal implementation**

```powershell
# append to scripts/lib/DeployDockerService.ps1

function Invoke-RemoteCommand {
    param(
        [Parameter(Mandatory)][string]$TargetHost,
        [Parameter(Mandatory)][string]$SshUser,
        [Parameter(Mandatory)][string]$SshKeyPath,
        [Parameter(Mandatory)][string]$Command
    )
    $sshArgs = @("-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-i", $SshKeyPath, "$SshUser@$TargetHost", $Command)
    $output = & ssh @sshArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Remote command failed (exit $LASTEXITCODE): $Command`n$output"
    }
    return $output
}

function Copy-ToRemote {
    param(
        [Parameter(Mandatory)][string]$TargetHost,
        [Parameter(Mandatory)][string]$SshUser,
        [Parameter(Mandatory)][string]$SshKeyPath,
        [Parameter(Mandatory)][string]$LocalPath,
        [Parameter(Mandatory)][string]$RemotePath
    )
    & scp -o "BatchMode=yes" -o "ConnectTimeout=10" -i $SshKeyPath $LocalPath "${SshUser}@${TargetHost}:${RemotePath}"
    if ($LASTEXITCODE -ne 0) {
        throw "File copy failed: $LocalPath -> ${TargetHost}:${RemotePath}"
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `Invoke-Pester scripts/tests/DeployDockerService.Tests.ps1`
Expected: PASS, all 11 tests green.

- [ ] **Step 5: Commit**

```bash
git add scripts/lib/DeployDockerService.ps1 scripts/tests/DeployDockerService.Tests.ps1
git commit -m "feat(deploy): add ssh/scp transport functions

The only two functions in the deploy library that touch the network --
every other function is pure or calls through these two, so the
orchestrator (next task) can be fully tested by mocking just these."
```

---

### Task 7: Deploy orchestrator with automatic rollback

**Files:**
- Modify: `scripts/lib/DeployDockerService.ps1`
- Modify: `scripts/tests/DeployDockerService.Tests.ps1`

**Interfaces:**
- Consumes: `Get-CurrentImageTag`, `Set-ImageTag` (Task 4), `Test-ServiceHealthy` (Task 5),
  `Invoke-RemoteCommand`, `Copy-ToRemote` (Task 6).
- Produces: `Invoke-DockerServiceDeploy(-Service ["central-server"|"web-ui"], -TargetHost,
  -SshUser, -SshKeyPath, -ImageTarLocalPath, -NewImageRef, -HealthUrl, -RemoteComposeDir =
  "C:/ifascada-central", -HealthMaxAttempts = 30, -HealthPollIntervalSeconds = 2)`. Throws on
  unrecoverable failure (deploy failed AND rollback failed); returns normally on success (deploy
  healthy) or on a successful rollback (logs the failure but does not throw, since the system is
  back in a known-good state) — consumed by Task 8's CLI wrapper.

- [ ] **Step 1: Write the failing tests**

```powershell
# append to scripts/tests/DeployDockerService.Tests.ps1

Describe "Invoke-DockerServiceDeploy" {
    BeforeEach {
        Mock Copy-ToRemote {}
        Mock Invoke-RemoteCommand {
            param($TargetHost, $SshUser, $SshKeyPath, $Command)
            if ($Command -like "type*") { return "CENTRAL_IMAGE=ifascada/central-server:1.0.2" }
            return ""
        }
    }

    It "deploys successfully and does not roll back when the health check passes" {
        Mock Test-ServiceHealthy { $true }

        Invoke-DockerServiceDeploy -Service "central-server" -TargetHost "192.168.103.154" `
            -SshUser "ifa" -SshKeyPath "C:\key" -ImageTarLocalPath "C:\image.tar" `
            -NewImageRef "ifascada/central-server:1.0.3" -HealthUrl "http://192.168.103.154:8088/health/live"

        Assert-MockCalled Copy-ToRemote -Times 1 -Exactly
        Assert-MockCalled Test-ServiceHealthy -Times 1 -Exactly
        Assert-MockCalled Invoke-RemoteCommand -ParameterFilter { $Command -like "*1.0.3*" } -Times 1
    }

    It "rolls back to the previous image when the health check fails, then succeeds" {
        Mock Test-ServiceHealthy { $false } -ParameterFilter { $true } -Verifiable
        $script:healthCallCount = 0
        Mock Test-ServiceHealthy {
            $script:healthCallCount++
            return $script:healthCallCount -ge 2
        }

        Invoke-DockerServiceDeploy -Service "central-server" -TargetHost "192.168.103.154" `
            -SshUser "ifa" -SshKeyPath "C:\key" -ImageTarLocalPath "C:\image.tar" `
            -NewImageRef "ifascada/central-server:1.0.3" -HealthUrl "http://192.168.103.154:8088/health/live"

        Assert-MockCalled Test-ServiceHealthy -Times 2 -Exactly
        Assert-MockCalled Invoke-RemoteCommand -ParameterFilter { $Command -like "*1.0.2*" } -Times 1
    }

    It "throws when both the deploy and the rollback fail health checks" {
        Mock Test-ServiceHealthy { $false }

        { Invoke-DockerServiceDeploy -Service "central-server" -TargetHost "192.168.103.154" `
            -SshUser "ifa" -SshKeyPath "C:\key" -ImageTarLocalPath "C:\image.tar" `
            -NewImageRef "ifascada/central-server:1.0.3" -HealthUrl "http://192.168.103.154:8088/health/live" } | Should Throw
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `Invoke-Pester scripts/tests/DeployDockerService.Tests.ps1`
Expected: FAIL — `Invoke-DockerServiceDeploy` is not defined.

- [ ] **Step 3: Write the minimal implementation**

```powershell
# append to scripts/lib/DeployDockerService.ps1

function Invoke-DockerServiceDeploy {
    param(
        [Parameter(Mandatory)][ValidateSet("central-server", "web-ui")][string]$Service,
        [Parameter(Mandatory)][string]$TargetHost,
        [Parameter(Mandatory)][string]$SshUser,
        [Parameter(Mandatory)][string]$SshKeyPath,
        [Parameter(Mandatory)][string]$ImageTarLocalPath,
        [Parameter(Mandatory)][string]$NewImageRef,
        [Parameter(Mandatory)][string]$HealthUrl,
        [string]$RemoteComposeDir = "C:/ifascada-central",
        [int]$HealthMaxAttempts = 30,
        [int]$HealthPollIntervalSeconds = 2
    )

    $envVarName = if ($Service -eq "central-server") { "CENTRAL_IMAGE" } else { "WEB_UI_IMAGE" }
    $remoteTarPath = "$RemoteComposeDir/deploy-$Service.tar"
    $remoteEnvPath = "$RemoteComposeDir/.env"

    Write-Host "[$Service] Copying image tar to $TargetHost..."
    Copy-ToRemote -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath -LocalPath $ImageTarLocalPath -RemotePath $remoteTarPath

    Write-Host "[$Service] Loading image on remote host..."
    Invoke-RemoteCommand -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath -Command "docker load -i $remoteTarPath" | Out-Null

    $currentEnvContent = (Invoke-RemoteCommand -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath -Command "type `"$remoteEnvPath`"") -join "`r`n"
    $previousImageRef = Get-CurrentImageTag -EnvContent $currentEnvContent -VarName $envVarName
    Write-Host "[$Service] Current image: $previousImageRef -> deploying: $NewImageRef"

    function Set-RemoteImageRefAndRestart([string]$ImageRef) {
        $newEnvContent = Set-ImageTag -EnvContent $currentEnvContent -VarName $envVarName -NewValue $ImageRef
        $escaped = $newEnvContent -replace '"', '""'
        Invoke-RemoteCommand -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath `
            -Command "powershell -NoProfile -Command `"Set-Content -Path '$remoteEnvPath' -Value \`"$escaped\`" -NoNewline`"" | Out-Null
        Invoke-RemoteCommand -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath `
            -Command "cd $RemoteComposeDir && docker compose up -d $Service" | Out-Null
    }

    Set-RemoteImageRefAndRestart -ImageRef $NewImageRef

    Write-Host "[$Service] Waiting for health check at $HealthUrl..."
    $healthy = Test-ServiceHealthy -Url $HealthUrl -MaxAttempts $HealthMaxAttempts -PollIntervalSeconds $HealthPollIntervalSeconds

    if ($healthy) {
        Write-Host "[$Service] Deploy succeeded: $NewImageRef is healthy."
        return
    }

    Write-Host "[$Service] Health check FAILED. Rolling back to $previousImageRef..."
    Set-RemoteImageRefAndRestart -ImageRef $previousImageRef
    $rolledBack = Test-ServiceHealthy -Url $HealthUrl -MaxAttempts $HealthMaxAttempts -PollIntervalSeconds $HealthPollIntervalSeconds
    if (-not $rolledBack) {
        throw "[$Service] Rollback to $previousImageRef ALSO failed health check. Manual intervention required."
    }
    throw "[$Service] Deploy of $NewImageRef failed health check; automatically rolled back to $previousImageRef."
}
```

Note: the second and third tests both expect a thrown error to signal "the deploy needs attention"
even on a successful rollback — this is intentional: a successful automatic rollback still means
the *new* version never shipped, and Task 12/13's workflow step must fail the GitHub Actions job in
both cases so a human notices. Update the docstring-equivalent comment above the function if this
surprises a future reader.

- [ ] **Step 4: Run tests to verify they pass**

Run: `Invoke-Pester scripts/tests/DeployDockerService.Tests.ps1`
Expected: PASS, all 14 tests green.

- [ ] **Step 5: Commit**

```bash
git add scripts/lib/DeployDockerService.ps1 scripts/tests/DeployDockerService.Tests.ps1
git commit -m "feat(deploy): add Invoke-DockerServiceDeploy orchestrator with auto-rollback

Implements Update Protocol v1 from the CI/CD spec: load image, swap
the .env-referenced tag, restart via docker compose, poll health, and
roll back automatically (throwing either way, so a CI job always fails
loudly on a bad deploy) if the new version doesn't come up healthy."
```

---

### Task 8: CLI entrypoint script

**Files:**
- Create: `scripts/deploy-docker-service.ps1`

**Interfaces:**
- Consumes: `Invoke-DockerServiceDeploy` (Task 7).
- Produces: the command line invoked by Task 12/13's GitHub Actions workflows.

- [ ] **Step 1: Write the entrypoint**

```powershell
# scripts/deploy-docker-service.ps1
param(
    [Parameter(Mandatory)][ValidateSet("central-server", "web-ui")][string]$Service,
    [Parameter(Mandatory)][string]$TargetHost,
    [Parameter(Mandatory)][string]$SshUser,
    [Parameter(Mandatory)][string]$SshKeyPath,
    [Parameter(Mandatory)][string]$ImageTarLocalPath,
    [Parameter(Mandatory)][string]$NewImageRef,
    [Parameter(Mandatory)][string]$HealthUrl,
    [string]$RemoteComposeDir = "C:/ifascada-central",
    [int]$HealthMaxAttempts = 30,
    [int]$HealthPollIntervalSeconds = 2
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "lib\DeployDockerService.ps1")

Invoke-DockerServiceDeploy -Service $Service -TargetHost $TargetHost -SshUser $SshUser `
    -SshKeyPath $SshKeyPath -ImageTarLocalPath $ImageTarLocalPath -NewImageRef $NewImageRef `
    -HealthUrl $HealthUrl -RemoteComposeDir $RemoteComposeDir `
    -HealthMaxAttempts $HealthMaxAttempts -HealthPollIntervalSeconds $HealthPollIntervalSeconds
```

- [ ] **Step 2: Verify parameter binding with `-WhatIf`-style dry check**

Run (this will actually fail past the parameter-binding stage since `C:\nonexistent.tar` doesn't
exist — that's expected and fine, it confirms the script parses and dispatches correctly):
```powershell
scripts/deploy-docker-service.ps1 -Service central-server -TargetHost 192.168.103.154 -SshUser ifa -SshKeyPath C:\nonexistent-key -ImageTarLocalPath C:\nonexistent.tar -NewImageRef "test:1.0.0" -HealthUrl "http://example.invalid"
```
Expected: fails with a `scp`/file-not-found-style error from inside `Copy-ToRemote`, **not** a
PowerShell parameter-binding error. That confirms all parameters wired through correctly.

- [ ] **Step 3: Commit**

```bash
git add scripts/deploy-docker-service.ps1
git commit -m "feat(deploy): add CLI entrypoint for the docker service deploy protocol"
```

---

### Task 9: Dedicated CI deploy SSH key

**No code — infrastructure setup.** Verification replaces the usual test steps.

- [ ] **Step 1: Generate the key** (run on the runner machine, `.193`)

```powershell
ssh-keygen -t ed25519 -f "$env:USERPROFILE\.ssh\ifascada_ci_deploy" -N '""' -C "ifascada-ci-deploy"
```

- [ ] **Step 2: Authorize the public key on `.154`**

Print the public key and copy it:
```powershell
Get-Content "$env:USERPROFILE\.ssh\ifascada_ci_deploy.pub"
```
On `.154`, append it to the `ifa` user's `authorized_keys` (same mechanism used for the personal
keys authorized during today's migration):
```powershell
Add-Content -Path "$env:USERPROFILE\.ssh\authorized_keys" -Value "<paste the public key here>"
```

- [ ] **Step 3: Verify the key works, and only for what it should**

From `.193`:
```powershell
ssh -o BatchMode=yes -i "$env:USERPROFILE\.ssh\ifascada_ci_deploy" ifa@192.168.103.154 "whoami"
```
Expected: succeeds, prints the `ifa` user. This key is never copied to, or authorized on, any edge
machine — there is nothing further to verify on that front because nothing was done there.

- [ ] **Step 4: Record the key path as a fixed convention**

No secret storage needed since the runner is a persistent, trusted machine (not ephemeral/hosted) —
document the path `$env:USERPROFILE\.ssh\ifascada_ci_deploy` in the workflow files directly (Tasks
12/13 reference it literally). No commit for this task (nothing goes in version control — the
private key never should).

---

### Task 10: Register the self-hosted GitHub Actions runner

**No code — infrastructure setup.**

- [ ] **Step 1: Add the runner from the GitHub UI**

In the `hochkofler/ifascada` repository: Settings -> Actions -> Runners -> "New self-hosted runner",
select Windows/x64, and follow the generated `config.cmd`/`run.cmd` commands exactly as GitHub
displays them (they embed a short-lived registration token, so copy them fresh rather than reusing
ones written down earlier).

- [ ] **Step 2: Run the generated commands on `.193`**

Follow GitHub's exact `Invoke-WebRequest`/`config.cmd`/`run.cmd` sequence as displayed. When
prompted for labels during `config.cmd`, add `ifascada` as an extra label (used by this plan's
workflows as `runs-on: [self-hosted, windows, ifascada]`).

- [ ] **Step 3: Install it as a persistent Windows service (not a foreground console session)**

```powershell
.\svc.cmd install
.\svc.cmd start
```
(Run from the runner installation directory — `svc.cmd` is the standard script GitHub's Windows
runner package includes for running unattended, across reboots, as a Windows service rather than a
foreground console session.)

- [ ] **Step 4: Verify**

In the repository's Settings -> Actions -> Runners page, confirm the new runner shows status
**Idle** (not offline). No commit for this task.

---

### Task 11: GitHub Environment `production` with a required reviewer

**No code — infrastructure setup.**

- [ ] **Step 1: Create the environment**

In the `hochkofler/ifascada` repository: Settings -> Environments -> "New environment", name it
`production`.

- [ ] **Step 2: Add the required-reviewer protection rule**

Under "Deployment protection rules", enable "Required reviewers" and add yourself (or whoever
should approve production deploys) as a required reviewer.

- [ ] **Step 3: Verify**

Settings -> Environments -> `production` shows the required-reviewer rule listed. No commit for
this task.

---

### Task 12: `central-server` GitHub Actions workflow

**Files:**
- Create: `.github/workflows/central-server.yml`

**Interfaces:**
- Consumes: `central-server/Dockerfile` (Task 1), `scripts/deploy-docker-service.ps1` (Task 8), the
  dedicated key at `$env:USERPROFILE\.ssh\ifascada_ci_deploy` (Task 9), the registered runner (Task
  10), the `production` environment (Task 11).

- [ ] **Step 1: Write the workflow**

```yaml
# .github/workflows/central-server.yml
name: central-server

on:
  push:
    tags:
      - 'central-v*'

jobs:
  build:
    runs-on: [self-hosted, windows, ifascada]
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - uses: actions/checkout@v4

      - name: Extract version from tag
        id: version
        shell: pwsh
        run: |
          $tag = "${{ github.ref_name }}"
          $version = $tag -replace '^central-v', ''
          "version=$version" >> $env:GITHUB_OUTPUT

      - name: Run tests
        shell: pwsh
        run: |
          cargo check --package central-server
          cargo test --package central-server

      - name: Build Docker image
        shell: pwsh
        run: |
          docker build -f central-server/Dockerfile -t ifascada/central-server:${{ steps.version.outputs.version }} .

      - name: Save image to tar
        shell: pwsh
        run: |
          docker save ifascada/central-server:${{ steps.version.outputs.version }} -o central-server-${{ steps.version.outputs.version }}.tar

      - name: Publish GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.ref_name }}
          files: central-server-${{ steps.version.outputs.version }}.tar

  deploy:
    needs: build
    runs-on: [self-hosted, windows, ifascada]
    environment: production
    steps:
      - name: Download release artifact
        shell: pwsh
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release download ${{ github.ref_name }} --pattern "*.tar" --dir .

      - name: Deploy to central host
        shell: pwsh
        run: |
          ./scripts/deploy-docker-service.ps1 `
            -Service "central-server" `
            -TargetHost "192.168.103.154" `
            -SshUser "ifa" `
            -SshKeyPath "$env:USERPROFILE\.ssh\ifascada_ci_deploy" `
            -ImageTarLocalPath "central-server-${{ needs.build.outputs.version }}.tar" `
            -NewImageRef "ifascada/central-server:${{ needs.build.outputs.version }}" `
            -HealthUrl "http://192.168.103.154:8088/health/live"
```

- [ ] **Step 2: Validate the YAML syntax**

```powershell
Get-Content .github/workflows/central-server.yml | ConvertFrom-Yaml
```
(If `ConvertFrom-Yaml` isn't available, `pwsh -Command "Get-Content .github/workflows/central-server.yml -Raw"` followed by a visual check, or GitHub's own web editor, which flags YAML syntax errors before allowing a commit through its UI — either way, confirm no syntax errors before relying on a real tag push to find them.)

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/central-server.yml
git commit -m "ci(central-server): add tag-triggered build/test/release/deploy workflow"
```

---

### Task 13: `web-ui` GitHub Actions workflow

**Files:**
- Create: `.github/workflows/web-ui.yml`

**Interfaces:**
- Consumes: `web-ui/Dockerfile` (Task 2), `/api/health` (Task 3), `scripts/deploy-docker-service.ps1`
  (Task 8), same runner/key/environment as Task 12.

- [ ] **Step 1: Write the workflow**

```yaml
# .github/workflows/web-ui.yml
name: web-ui

on:
  push:
    tags:
      - 'webui-v*'

jobs:
  build:
    runs-on: [self-hosted, windows, ifascada]
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - uses: actions/checkout@v4

      - name: Extract version from tag
        id: version
        shell: pwsh
        run: |
          $tag = "${{ github.ref_name }}"
          $version = $tag -replace '^webui-v', ''
          "version=$version" >> $env:GITHUB_OUTPUT

      - name: Install dependencies and build
        shell: pwsh
        working-directory: web-ui
        run: |
          npm ci
          npm run build

      - name: Build Docker image
        shell: pwsh
        run: |
          docker build -f web-ui/Dockerfile -t ifascada/web-ui:${{ steps.version.outputs.version }} .

      - name: Save image to tar
        shell: pwsh
        run: |
          docker save ifascada/web-ui:${{ steps.version.outputs.version }} -o web-ui-${{ steps.version.outputs.version }}.tar

      - name: Publish GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.ref_name }}
          files: web-ui-${{ steps.version.outputs.version }}.tar

  deploy:
    needs: build
    runs-on: [self-hosted, windows, ifascada]
    environment: production
    steps:
      - name: Download release artifact
        shell: pwsh
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release download ${{ github.ref_name }} --pattern "*.tar" --dir .

      - name: Deploy to central host
        shell: pwsh
        run: |
          ./scripts/deploy-docker-service.ps1 `
            -Service "web-ui" `
            -TargetHost "192.168.103.154" `
            -SshUser "ifa" `
            -SshKeyPath "$env:USERPROFILE\.ssh\ifascada_ci_deploy" `
            -ImageTarLocalPath "web-ui-${{ needs.build.outputs.version }}.tar" `
            -NewImageRef "ifascada/web-ui:${{ needs.build.outputs.version }}" `
            -HealthUrl "http://192.168.103.154:3001/api/health"
```

- [ ] **Step 2: Validate the YAML syntax**

```powershell
Get-Content .github/workflows/web-ui.yml | ConvertFrom-Yaml
```
(If `ConvertFrom-Yaml` isn't available, use GitHub's own web editor for this file, which flags YAML
syntax errors before allowing a commit through its UI — either way, confirm no syntax errors before
relying on a real tag push to find them.)

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/web-ui.yml
git commit -m "ci(web-ui): add tag-triggered build/test/release/deploy workflow"
```

---

### Task 14: `edge-agent` GitHub Actions workflow (build/release only, no deploy)

**Files:**
- Create: `.github/workflows/edge-agent.yml`

**Interfaces:**
- Consumes: the existing `deploy/edge-1.0.0-runtime/scripts/build-edge-package.ps1` (unchanged, per
  the spec's Non-Goals), same runner as Tasks 12/13. **No deploy job** — per the spec, edges pull
  their own updates (separate plan); this workflow only publishes the artifact they'll eventually
  pull.

- [ ] **Step 1: Write the workflow**

```yaml
# .github/workflows/edge-agent.yml
name: edge-agent

on:
  push:
    tags:
      - 'edge-v*'

jobs:
  build:
    runs-on: [self-hosted, windows, ifascada]
    steps:
      - uses: actions/checkout@v4

      - name: Extract version from tag
        id: version
        shell: pwsh
        run: |
          $tag = "${{ github.ref_name }}"
          $version = $tag -replace '^edge-v', ''
          "version=$version" >> $env:GITHUB_OUTPUT

      - name: Run tests
        shell: pwsh
        run: |
          cargo check --package edge-agent
          cargo test --package edge-agent

      - name: Build release binary
        shell: pwsh
        run: |
          cargo build --release --package edge-agent

      - name: Build release package
        shell: pwsh
        run: |
          ./deploy/edge-1.0.0-runtime/scripts/build-edge-package.ps1 `
            -BinaryPath "target/release/edge-agent.exe" `
            -OutputRoot "edge-package" `
            -Version "${{ steps.version.outputs.version }}" `
            -ConfigSchemaVersion 1 `
            -MinimumCentralVersion "1.0.0"

      - name: Zip the package
        shell: pwsh
        run: |
          Compress-Archive -Path "edge-package/*" -DestinationPath "edge-agent-${{ steps.version.outputs.version }}.zip"

      - name: Publish GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.ref_name }}
          files: edge-agent-${{ steps.version.outputs.version }}.zip
```

- [ ] **Step 2: Validate the YAML syntax**

```powershell
Get-Content .github/workflows/edge-agent.yml | ConvertFrom-Yaml
```
(If `ConvertFrom-Yaml` isn't available, use GitHub's own web editor for this file, which flags YAML
syntax errors before allowing a commit through its UI — either way, confirm no syntax errors before
relying on a real tag push to find them.)

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/edge-agent.yml
git commit -m "ci(edge-agent): add tag-triggered build/test/release workflow (no deploy)

No deploy job here by design -- edges pull their own updates (separate
plan). This workflow's only job is to give central-server (and,
eventually, edges) a real published artifact to point at."
```

---

### Task 15: End-to-end validation

**No new files — this task proves the whole pipeline works together.**

- [ ] **Step 1: Pick a real next version and tag it**

```bash
git tag central-v1.0.3
git push origin central-v1.0.3
```

- [ ] **Step 2: Watch the build job**

In the repository's Actions tab, confirm the `central-server` workflow's `build` job runs on the
`ifascada` runner, passes `cargo check`/`cargo test`, and publishes a GitHub Release with the
`.tar` artifact attached.

- [ ] **Step 3: Approve the deploy**

The `deploy` job should pause waiting for the `production` environment's required reviewer. Approve
it from the Actions UI.

- [ ] **Step 4: Confirm the deploy succeeded**

Watch the `deploy` job logs for `[central-server] Deploy succeeded: ifascada/central-server:1.0.3
is healthy.` Then independently confirm from any machine on the network:
```powershell
Invoke-WebRequest http://192.168.103.154:8088/health/live -UseBasicParsing
```
Expected: `200 {"status":"ok"}`, and `docker images` on `.154` shows both `1.0.2` (kept for manual
rollback) and the new `1.0.3` tags present.

- [ ] **Step 5: Repeat Steps 1–4 for `web-ui`**

```bash
git tag webui-v1.0.3
git push origin webui-v1.0.3
```
Same verification, against `http://192.168.103.154:3001/api/health` and `docker images` showing
both `web-ui` tags.

- [ ] **Step 6: Deliberately verify the rollback path once**

Temporarily point `-HealthUrl` at a URL that will never return 200 (e.g. append a nonexistent path
like `/health/live-typo-on-purpose`) in a throwaway local run of `scripts/deploy-docker-service.ps1`
against a **non-production** image tag, and confirm: the job's console output shows the "Health
check FAILED. Rolling back..." message, the previous image gets restored, and the script still
exits with a nonzero code (throws) even though the rollback itself succeeded — confirming a human
would see this deploy as failed in the Actions UI, not as a silent success.

This task has no commit of its own — it's a verification pass over everything committed in Tasks
1–14.
