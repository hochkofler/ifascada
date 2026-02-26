param(
    [string]$Site = "plant-a",
    [string]$Agent = "edge-01",
    [string]$MqttHost = "127.0.0.1",
    [int]$MqttPort = 1883,
    [string]$PgDsn = "host=127.0.0.1 user=postgres password=postgres dbname=ifascada",
    [string]$PgUrl = "",
    [bool]$EnableRedis = $false,
    [string]$RedisUrl = "redis://127.0.0.1:6379/",
    [int]$WarmupSeconds = 4,
    [int]$CaptureSeconds = 4
)

$ErrorActionPreference = "Stop"

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

# Ensure Mosquitto path is available in common Windows install.
if (Test-Path "C:\Program Files\Mosquitto") {
    $env:Path = "C:\Program Files\Mosquitto;" + $env:Path
}

Require-Command "cargo"
Require-Command "mosquitto_pub"

$hasPsql = $null -ne (Get-Command psql -ErrorAction SilentlyContinue)

$topicTelemetry = "scada/$Site/edge/$Agent/telemetry/tag/tag_hr_0"
$topicHealth = "scada/$Site/edge/$Agent/health/runtime"
$topicAck = "scada/$Site/edge/$Agent/cmd/write/ack"
$topicAudit = "scada/$Site/edge/$Agent/audit/write"

$logCentral = New-LogPath "central-server.log"
$logCentralErr = New-LogPath "central-server.err.log"
$payloadTelemetryFile = New-LogPath "central-telemetry.json"
$payloadHealthFile = New-LogPath "central-health.json"
$payloadAckFile = New-LogPath "central-ack.json"
$payloadAuditFile = New-LogPath "central-audit.json"

Remove-Item -ErrorAction SilentlyContinue $logCentral, $logCentralErr, $payloadTelemetryFile, $payloadHealthFile, $payloadAckFile, $payloadAuditFile

$payloadTelemetry = '{"schema_version":1,"source":"edge-agent","tag_id":"tag_hr_0","value":42.5,"quality":{"status":"Good","reason":"None"},"timestamp":"2026-02-21T14:00:00Z"}'
$payloadHealth = '{"schema_version":1,"source":"edge-agent","status":"ok","outbox_depth":0,"outbox_oldest_age_secs":null,"timestamp":"2026-02-21T14:00:01Z"}'
$payloadAck = '{"schema_version":1,"source":"edge/edge-01","tag_id":"tag_hr_0","command_id":"cmd-central-e2e-1","success":true,"reason":null,"timestamp":"2026-02-21T14:00:02Z"}'
$payloadAudit = '{"schema_version":1,"source":"edge/edge-01","connection_id":"conn_modbus_tcp_1","tag_id":"tag_hr_0","command_id":"cmd-central-e2e-1","value":42.5,"outcome":"Applied","reason":null,"timestamp":"2026-02-21T14:00:03Z"}'

[System.IO.File]::WriteAllText($payloadTelemetryFile, $payloadTelemetry, (New-Object System.Text.UTF8Encoding($false)))
[System.IO.File]::WriteAllText($payloadHealthFile, $payloadHealth, (New-Object System.Text.UTF8Encoding($false)))
[System.IO.File]::WriteAllText($payloadAckFile, $payloadAck, (New-Object System.Text.UTF8Encoding($false)))
[System.IO.File]::WriteAllText($payloadAuditFile, $payloadAudit, (New-Object System.Text.UTF8Encoding($false)))

$procs = @()
try {
    Write-Host "Starting central-server..."
    $env:CENTRAL_MQTT_ENABLED = "true"
    $env:MQTT_HOST = $MqttHost
    $env:MQTT_PORT = "$MqttPort"
    $env:CENTRAL_PG_DSN = $PgDsn
    $env:CENTRAL_REDIS_ENABLED = if ($EnableRedis) { "true" } else { "false" }
    $env:CENTRAL_REDIS_URL = $RedisUrl
    $env:RUST_LOG = "info"

    $procs += Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $logCentral -RedirectStandardError $logCentralErr -ArgumentList @("run","-p","central-server")

    Write-Host "Warmup $WarmupSeconds seconds..."
    Start-Sleep -Seconds $WarmupSeconds

    Write-Host "Publishing test messages to MQTT..."
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $topicTelemetry -f $payloadTelemetryFile
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $topicHealth -f $payloadHealthFile
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $topicAck -f $payloadAckFile
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $topicAudit -f $payloadAuditFile

    Write-Host "Capturing for $CaptureSeconds seconds..."
    Start-Sleep -Seconds $CaptureSeconds

    Write-Host ""
    Write-Host "===== CENTRAL ERR (tail) ====="
    if (Test-Path $logCentralErr) { Get-Content $logCentralErr | Select-Object -Last 60 }

    if ($hasPsql) {
        Write-Host ""
        Write-Host "===== POSTGRES COUNTS ====="
        $conn = if ([string]::IsNullOrWhiteSpace($PgUrl)) { $PgDsn } else { $PgUrl }
        $sql = @"
SELECT 'telemetry_ingest_events' AS table_name, COUNT(*)::bigint AS c FROM telemetry_ingest_events
UNION ALL
SELECT 'telemetry_samples', COUNT(*)::bigint FROM telemetry_samples
UNION ALL
SELECT 'edge_health_events', COUNT(*)::bigint FROM edge_health_events
UNION ALL
SELECT 'command_ack_events', COUNT(*)::bigint FROM command_ack_events
UNION ALL
SELECT 'command_audit_events', COUNT(*)::bigint FROM command_audit_events
UNION ALL
SELECT 'tag_current_state', COUNT(*)::bigint FROM tag_current_state
UNION ALL
SELECT 'edge_current_state', COUNT(*)::bigint FROM edge_current_state
ORDER BY table_name;
"@
        & psql "$conn" --no-password -v ON_ERROR_STOP=1 -c $sql
    } else {
        Write-Host "psql not found in PATH; skipping DB verification output."
    }

    Write-Host ""
    Write-Host "Logs saved in data/e2e/"
}
finally {
    Write-Host "Stopping spawned processes..."
    foreach ($p in $procs) {
        if ($null -ne $p -and -not $p.HasExited) {
            try { Stop-Process -Id $p.Id -Force } catch {}
        }
    }
}
