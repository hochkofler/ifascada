param(
    [string]$Site = "plant-a",
    [string]$Agent = "edge-01",
    [string]$MqttHost = "127.0.0.1",
    [int]$MqttPort = 51883,
    [string]$PgDsn = "host=127.0.0.1 port=55432 user=postgres dbname=rustscada sslmode=disable",
    [string]$CentralBind = "127.0.0.1:18088"
)

$ErrorActionPreference = "Stop"

# Ensure Mosquitto path is available in common Windows install.
if (Test-Path "C:\Program Files\Mosquitto") {
    $env:Path = "C:\Program Files\Mosquitto;" + $env:Path
}

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

function New-LogPath([string]$Name) {
    $dir = "data/e2e"
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    return Join-Path $dir $Name
}

function Wait-Until([scriptblock]$Condition, [int]$TimeoutSeconds, [string]$FailMessage) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 500
    }
    throw $FailMessage
}

Require-Command "cargo"
Require-Command "mosquitto_sub"
Require-Command "mosquitto_pub"
if (-not (Get-Command psql -ErrorAction SilentlyContinue)) {
    throw "Missing required command 'psql'. Install it and retry."
}

$procs = @()
$logCentralOut = New-LogPath "cfg-apply-central.out.log"
$logCentralErr = New-LogPath "cfg-apply-central.err.log"
$logEdgeOut = New-LogPath "cfg-apply-edge.out.log"
$logEdgeErr = New-LogPath "cfg-apply-edge.err.log"
$logHealth = New-LogPath "cfg-apply-health.log"
$logApplyRs = New-LogPath "cfg-apply-result.log"
$runtimeA = New-LogPath "runtime-config-A.json"
$runtimeB = New-LogPath "runtime-config-B.json"
$cachePath = New-LogPath "runtime-config-cache.signed.json"
$receiptPath = New-LogPath "runtime-config-apply-receipt.json"
Remove-Item -ErrorAction SilentlyContinue $logCentralOut, $logCentralErr, $logEdgeOut, $logEdgeErr, $logHealth, $logApplyRs, $runtimeA, $runtimeB, $cachePath, $receiptPath

try {
    # Migrations (idempotent)
    $migs = @(
        "crates/central-server/migrations/0001_core_postgres.sql",
        "crates/central-server/migrations/0002_timescale_historian.sql",
        "crates/central-server/migrations/0003_tag_naming_governance.sql",
        "crates/central-server/migrations/0005_fix_tag_naming_constraint_regex.sql",
        "crates/central-server/migrations/0006_context_hierarchy.sql",
        "crates/central-server/migrations/0004_dev_seed_minimal_catalog.sql",
        "crates/central-server/migrations/0007_dev_seed_context_hierarchy.sql",
        "crates/central-server/migrations/0008_dev_seed_sim20_multi_area.sql",
        "crates/central-server/migrations/0009_operational_events.sql",
        "crates/central-server/migrations/0010_connection_domain_state.sql",
        "crates/central-server/migrations/0011_device_domain_state.sql",
        "crates/central-server/migrations/0012_edges_metadata_json.sql",
        "crates/central-server/migrations/0016_telemetry_received_at.sql",
        "crates/central-server/migrations/0013_scale_manual_config_in_catalog.sql"
    )
    foreach ($m in $migs) {
        psql "$PgDsn" -v ON_ERROR_STOP=1 -f "$m" | Out-Null
    }

    # Build two runtime config variants (different hash)
    $baseCfg = Get-Content "crates/edge-agent/config/bootstrap.example.json" -Raw
    [System.IO.File]::WriteAllText($runtimeA, $baseCfg, (New-Object System.Text.UTF8Encoding($false)))
    $cfgB = $baseCfg -replace '"name"\s*:\s*"Modbus TCP Demo"', '"name": "Modbus TCP Demo V2"'
    [System.IO.File]::WriteAllText($runtimeB, $cfgB, (New-Object System.Text.UTF8Encoding($false)))

    Write-Host "Starting central-server (API+MQTT)..."
    $env:CENTRAL_PG_DSN = $PgDsn
    $env:CENTRAL_MQTT_ENABLED = "true"
    $env:MQTT_HOST = $MqttHost
    $env:MQTT_PORT = "$MqttPort"
    $env:CENTRAL_MQTT_CLIENT_ID = "central-server-e2e-config"
    $env:CENTRAL_MQTT_TOPIC_FILTER = "scada/+/edge/+/#"
    $env:CENTRAL_API_ENABLED = "true"
    $env:CENTRAL_API_BIND = $CentralBind
    $env:CENTRAL_EDGE_ENROLL_TOKEN = "dev-edge-enroll-token"
    $env:CENTRAL_EDGE_CONFIG_SIGNING_SECRET = "dev-edge-config-signing-secret"
    $env:CENTRAL_EDGE_CONFIG_SIGNING_KEY_ID = "v1"
    $env:CENTRAL_EDGE_RUNTIME_CONFIG_PATH = $runtimeA
    $env:RUST_LOG = "info"
    $procs += Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $logCentralOut -RedirectStandardError $logCentralErr -ArgumentList @("run","-p","central-server")

    $baseUrl = "http://$CentralBind"
    Wait-Until -Condition {
        try {
            $r = Invoke-WebRequest -Uri "$baseUrl/health/live" -UseBasicParsing -TimeoutSec 2
            $r.StatusCode -eq 200
        } catch { $false }
    } -TimeoutSeconds 45 -FailMessage "central health timeout"

    $topicHealth = "scada/$Site/edge/$Agent/health/runtime"
    $topicApplyRs = "scada/$Site/edge/$Agent/config/apply/result"
    $topicApply = "scada/$Site/edge/$Agent/config/apply"
    Write-Host "Subscribing health/apply-result topics..."
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logHealth -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$topicHealth)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logApplyRs -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$topicApplyRs)

    Write-Host "Starting edge-agent (run #1)..."
    $env:EDGE_MQTT_ENABLED = "true"
    $env:EDGE_SITE = $Site
    $env:EDGE_AGENT = $Agent
    $env:EDGE_CONFIG_URL = $baseUrl
    $env:EDGE_ENROLL_TOKEN = "dev-edge-enroll-token"
    $env:EDGE_CONFIG_HMAC_SECRET = "dev-edge-config-signing-secret"
    $env:EDGE_CONFIG_KEY_ID = "v1"
    $env:EDGE_RUNTIME_CACHE_PATH = $cachePath
    $env:EDGE_CONFIG_APPLY_RECEIPT_PATH = $receiptPath
    $env:EDGE_CONFIG_CHECK_INTERVAL_SECS = "10"
    $env:EDGE_CONFIG_CHECK_JITTER_SECS = "0"
    $env:MQTT_HEALTH_PUBLISH_INTERVAL_SECS = "5"
    $env:MQTT_HOST = $MqttHost
    $env:MQTT_PORT = "$MqttPort"
    $edge1 = Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $logEdgeOut -RedirectStandardError $logEdgeErr -ArgumentList @("run","-p","edge-agent")
    $procs += $edge1

    Wait-Until -Condition {
        if (-not (Test-Path $logHealth)) { return $false }
        $raw = Get-Content $logHealth -Raw
        $raw -match '"config_sync_state":"in_sync"'
    } -TimeoutSeconds 160 -FailMessage "edge did not reach in_sync"

    Write-Host "Switching central runtime config to variant B (forces hash change)..."
    Copy-Item -Path $runtimeB -Destination $runtimeA -Force

    Wait-Until -Condition {
        if (-not (Test-Path $logHealth)) { return $false }
        $raw = Get-Content $logHealth -Raw
        $raw -match '"config_sync_state":"changed_staged"'
    } -TimeoutSeconds 160 -FailMessage "edge did not reach changed_staged"

    Write-Host "Publishing config/apply..."
    # Keep payload minimal to avoid shell quoting issues in different PowerShell environments.
    $applyPayload = '{}'
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $topicApply -m $applyPayload

    Wait-Until -Condition {
        $edge1.HasExited
    } -TimeoutSeconds 40 -FailMessage "edge did not exit after apply request"

    Write-Host "Starting edge-agent (run #2 after restart)..."
    $edge2 = Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $logEdgeOut -RedirectStandardError $logEdgeErr -ArgumentList @("run","-p","edge-agent")
    $procs += $edge2

    Wait-Until -Condition {
        if (-not (Test-Path $logApplyRs)) { return $false }
        $raw = Get-Content $logApplyRs -Raw
        $raw -match 'applied_after_restart'
    } -TimeoutSeconds 60 -FailMessage "did not receive applied_after_restart result"

    Write-Host ""
    Write-Host "E2E config apply/restart PASSED."
    Write-Host " - Health log: $logHealth"
    Write-Host " - Apply result log: $logApplyRs"
}
finally {
    Write-Host "Stopping spawned processes..."
    foreach ($p in $procs) {
        if ($null -ne $p -and -not $p.HasExited) {
            try { Stop-Process -Id $p.Id -Force } catch {}
        }
    }
}
