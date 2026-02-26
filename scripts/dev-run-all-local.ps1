param(
    [string]$ComposeFile = "docker-compose.scada.yml",
    [string]$SimComposeFile = "docker-compose.edge-sim.yml",
    [switch]$NoInfraStart,
    [switch]$StartSimEdges,
    [switch]$RebuildSimEdges,
    [string]$ComPort = "COM7",
    [string]$BootstrapPath = "crates/edge-agent/config/bootstrap.dev.manual-scale.json",
    [string]$UiPort = "3001",
    [ValidateSet("minimal", "sim20", "full")]
    [string]$SeedProfile = "minimal",
    [switch]$ResetSchema,
    [int]$AutoStopSeconds = 0
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

function Ensure-Path([string]$Path) {
    if (Test-Path $Path) { return }
    throw "File not found: $Path"
}

function Assert-PortFree([int]$Port, [string]$Name) {
    $inUse = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    if ($null -ne $inUse) {
        throw "$Name port $Port is already in use. Stop the process or pass another port."
    }
}

function Wait-HttpOk([string]$Url, [int]$TimeoutSeconds) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        try {
            $res = Invoke-WebRequest -Uri $Url -Method Get -UseBasicParsing -TimeoutSec 3
            if ($res.StatusCode -ge 200 -and $res.StatusCode -lt 500) {
                return
            }
        }
        catch {}
        Start-Sleep -Milliseconds 800
    }
    throw "Timeout waiting for $Url"
}

function Wait-PsqlReady([string]$Dsn, [int]$TimeoutSeconds) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        psql "$Dsn" -c "SELECT 1;" *> $null
        if ($LASTEXITCODE -eq 0) { return }
        Start-Sleep -Seconds 2
    }
    throw "Timeout waiting for Postgres readiness on DSN: $Dsn"
}

function Invoke-PsqlFile([string]$Dsn, [string]$File) {
    psql "$Dsn" -v ON_ERROR_STOP=1 -f "$File"
    if ($LASTEXITCODE -ne 0) {
        throw "psql migration failed: $File (exit code $LASTEXITCODE)"
    }
}

function New-LogDir() {
    $dir = "data/dev-run-all"
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    return $dir
}

Require-Command "docker"
Require-Command "cargo"
Require-Command "npm"
Ensure-Path $ComposeFile
Ensure-Path $BootstrapPath
if ($StartSimEdges) {
    Ensure-Path $SimComposeFile
}
Assert-PortFree -Port 8088 -Name "central-api"
Assert-PortFree -Port ([int]$UiPort) -Name "web-ui"

if (-not $NoInfraStart) {
    Write-Host "Starting infrastructure from $ComposeFile ..."
    docker compose -f $ComposeFile up -d
}

if ($StartSimEdges) {
    Write-Host "Starting simulator edge-agents in Docker from $SimComposeFile ..."
    $simArgs = @("-f", $ComposeFile, "-f", $SimComposeFile, "up", "-d")
    if ($RebuildSimEdges) {
        $simArgs += "--build"
    }
    $simArgs += @("edge-sim-pack-1", "edge-sim-pack-2", "edge-sim-mix-1", "edge-sim-mix-2")
    docker compose @simArgs
}

$env:MQTT_HOST = "127.0.0.1"
$env:MQTT_PORT = "51883"
$env:CENTRAL_MQTT_ENABLED = "true"
$env:CENTRAL_MQTT_CLIENT_ID = "central-server-01"
$env:CENTRAL_MQTT_TOPIC_FILTER = "scada/+/edge/+/#"
$env:CENTRAL_API_ENABLED = "true"
$env:CENTRAL_API_BIND = "127.0.0.1:8088"
$env:CENTRAL_PG_DSN = "host=127.0.0.1 port=55432 user=postgres dbname=rustscada sslmode=disable"
$env:CENTRAL_REDIS_ENABLED = "true"
$env:CENTRAL_REDIS_URL = "redis://127.0.0.1:56379/"
$env:CENTRAL_REDIS_EVENT_CHANNEL = "scada:rt:events"
$env:CENTRAL_REDIS_KEY_TTL_SECS = "300"
$env:EDGE_MQTT_ENABLED = "true"
$env:EDGE_SITE = "plant-a"
$env:EDGE_AGENT = "edge-com-01"
$env:EDGE_CONFIG_URL = "http://127.0.0.1:8088"
$env:EDGE_ENROLL_TOKEN = "dev-edge-enroll-token"
$env:EDGE_CONFIG_HMAC_SECRET = "dev-edge-config-signing-secret"
$env:EDGE_CONFIG_KEY_ID = "v1"
$env:EDGE_CONFIG_CHECK_INTERVAL_SECS = "20"
$env:EDGE_CONFIG_CHECK_JITTER_SECS = "0"
$env:NEXT_PUBLIC_API_BASE = "http://127.0.0.1:8088"
$env:NEXT_PUBLIC_SSE_URL = "http://127.0.0.1:8088/api/stream/events"

$logDir = New-LogDir
$env:EDGE_RUNTIME_CACHE_PATH = (Join-Path $logDir "runtime_config.edge-com-01.signed.json")
$env:EDGE_CONFIG_APPLY_RECEIPT_PATH = (Join-Path $logDir "config_apply_receipt.edge-com-01.json")
$centralOut = Join-Path $logDir "central.out.log"
$centralErr = Join-Path $logDir "central.err.log"
$edgeOut = Join-Path $logDir "edge.out.log"
$edgeErr = Join-Path $logDir "edge.err.log"
$uiOut = Join-Path $logDir "web-ui.out.log"
$uiErr = Join-Path $logDir "web-ui.err.log"
$bootstrapRuntime = Join-Path $logDir "bootstrap.runtime.json"
Remove-Item -ErrorAction SilentlyContinue $centralOut, $centralErr, $edgeOut, $edgeErr, $uiOut, $uiErr

$bootstrapRaw = Get-Content $BootstrapPath -Raw
$bootstrapRaw = $bootstrapRaw -replace '"port"\s*:\s*"COM\d+"', ('"port": "' + $ComPort + '"')
[System.IO.File]::WriteAllText($bootstrapRuntime, $bootstrapRaw, (New-Object System.Text.UTF8Encoding($false)))
$env:EDGE_BOOTSTRAP_PATH = $bootstrapRuntime

$hasPsql = [bool](Get-Command psql -ErrorAction SilentlyContinue)
if ($hasPsql) {
    Wait-PsqlReady -Dsn $env:CENTRAL_PG_DSN -TimeoutSeconds 90
    $resetSeedScript = Join-Path $PSScriptRoot "reset-db-and-seed.ps1"
    if (-not (Test-Path $resetSeedScript)) {
        throw "Missing reset/seed script: $resetSeedScript"
    }
    Write-Host "Applying central schema/seed via reset-db-and-seed.ps1 (profile=$SeedProfile, reset=$ResetSchema) ..."
    if ($ResetSchema) {
        & $resetSeedScript -PgDsn $env:CENTRAL_PG_DSN -ResetSchema -SeedProfile $SeedProfile
    } else {
        & $resetSeedScript -PgDsn $env:CENTRAL_PG_DSN -SeedProfile $SeedProfile
    }
} else {
    Write-Host "Warning: psql not found. Ensure central migrations are already applied."
}

$procs = @()
try {
    Write-Host "Starting central-server ..."
    $procs += Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $centralOut -RedirectStandardError $centralErr -ArgumentList @("run","-p","central-server")

    Write-Host "Starting edge-agent (manual scale port: $ComPort) ..."
    $procs += Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $edgeOut -RedirectStandardError $edgeErr -ArgumentList @("run","-p","edge-agent")

    if (-not (Test-Path "web-ui/node_modules")) {
        Write-Host "Installing web-ui dependencies ..."
        Push-Location "web-ui"
        try {
            npm install
        }
        finally {
            Pop-Location
        }
    }

    Write-Host "Starting web-ui on http://127.0.0.1:$UiPort ..."
    $procs += Start-Process npm.cmd -PassThru -NoNewWindow -WorkingDirectory "web-ui" -RedirectStandardOutput $uiOut -RedirectStandardError $uiErr -ArgumentList @("run","dev","--","-p",$UiPort)

    Wait-HttpOk -Url "http://127.0.0.1:8088/health/live" -TimeoutSeconds 120
    Wait-HttpOk -Url ("http://127.0.0.1:" + $UiPort) -TimeoutSeconds 120

    Write-Host ""
    Write-Host "Services started:"
    foreach ($p in $procs) { Write-Host (" - PID {0}" -f $p.Id) }
    Write-Host "Central API: http://127.0.0.1:8088/health/live"
    Write-Host "HMI:         http://127.0.0.1:$UiPort/live"
    Write-Host "Logs:        $logDir"
    if ($StartSimEdges) {
        Write-Host "Sim edges:   ifascada-edge-sim-pack-1, ifascada-edge-sim-pack-2, ifascada-edge-sim-mix-1, ifascada-edge-sim-mix-2"
        Write-Host "Sim logs:    docker logs -f ifascada-edge-sim-pack-1 (or pack-2/mix-1/mix-2)"
    }
    Write-Host ""
    Write-Host "Manual scale test on ${ComPort}:"
    Write-Host " - Send ASCII line ending with CR (or CRLF): + 12.4354 g"
    Write-Host " - Also accepted by regex: -0.120 kg,   8.0000   g"
    Write-Host ""
    if ($AutoStopSeconds -gt 0) {
        Write-Host "Auto stop in $AutoStopSeconds seconds..."
        Start-Sleep -Seconds $AutoStopSeconds
    } else {
        Write-Host "Press ENTER to stop all local processes..."
        [void](Read-Host)
    }
}
finally {
    Write-Host "Stopping processes..."
    foreach ($p in $procs) {
        if ($null -ne $p -and -not $p.HasExited) {
            try {
                # Kill full process tree (important for npm -> node child processes)
                taskkill /PID $p.Id /T /F | Out-Null
            } catch {
                try { Stop-Process -Id $p.Id -Force } catch {}
            }
        }
    }

    $verifyScript = Join-Path $PSScriptRoot "verify-stop-ports.ps1"
    if (Test-Path $verifyScript) {
        & $verifyScript -Ports @("8088", "$UiPort") -TimeoutSeconds 20
    }
}
