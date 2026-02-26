param(
    [string]$Site = "plant-a",
    [string]$Agent = "edge-replay-01",
    [string]$Tag = "tag_replay_001",
    [string]$MqttHost = "127.0.0.1",
    [int]$MqttPort = 51883,
    [string]$PgDsn = "host=127.0.0.1 port=55432 user=postgres dbname=rustscada sslmode=disable",
    [int]$WarmupSeconds = 3,
    [int]$DownPublishCount = 3
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'."
    }
}

function Invoke-PsqlFile([string]$Dsn, [string]$File) {
    psql "$Dsn" -v ON_ERROR_STOP=1 -f "$File"
    if ($LASTEXITCODE -ne 0) {
        throw "psql migration failed: $File"
    }
}

function Wait-CentralReady([string]$ErrLog, [int]$TimeoutSeconds = 25) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (Test-Path $ErrLog) {
            $tail = Get-Content $ErrLog -ErrorAction SilentlyContinue | Select-Object -Last 40
            if (($tail -join "`n") -match "subscribed to") { return }
        }
        Start-Sleep -Milliseconds 500
    }
    throw "Timeout waiting central MQTT subscription readiness."
}

Require-Command "cargo"
Require-Command "psql"
Require-Command "mosquitto_pub"

$env:CENTRAL_PG_DSN = $PgDsn
$env:CENTRAL_MQTT_ENABLED = "true"
$env:CENTRAL_API_ENABLED = "false"
$env:MQTT_HOST = $MqttHost
$env:MQTT_PORT = "$MqttPort"
$env:CENTRAL_MQTT_CLIENT_ID = "central-replay-e2e-01"
$env:CENTRAL_MQTT_CLEAN_SESSION = "false"
$env:CENTRAL_MQTT_TOPIC_FILTER = "scada/+/edge/+/#"
$env:RUST_LOG = "info"

$migrations = @(
    "crates/central-server/migrations/0001_core_postgres.sql",
    "crates/central-server/migrations/0002_timescale_historian.sql",
    "crates/central-server/migrations/0016_telemetry_received_at.sql"
)
foreach ($m in $migrations) { Invoke-PsqlFile -Dsn $PgDsn -File $m }

$dir = "data/e2e"
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
$logOut = Join-Path $dir "central-replay.out.log"
$logErr = Join-Path $dir "central-replay.err.log"
Remove-Item -ErrorAction SilentlyContinue $logOut, $logErr

$topic = "scada/$Site/edge/$Agent/telemetry/tag/$Tag"

function Publish-Telemetry([string]$IsoTs, [double]$Value) {
    $payload = "{`"schema_version`":1,`"source`":`"edge-agent`",`"tag_id`":`"$Tag`",`"value`":$Value,`"quality`":{`"status`":`"Good`",`"reason`":null},`"timestamp`":`"$IsoTs`"}"
    mosquitto_pub -h $MqttHost -p $MqttPort -q 1 -t $topic -m $payload
}

$central = $null
try {
    Write-Host "Starting central (phase 1)..."
    $central = Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $logOut -RedirectStandardError $logErr -ArgumentList @("run","-p","central-server")
    Wait-CentralReady -ErrLog $logErr
    Start-Sleep -Seconds $WarmupSeconds

    $t0 = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
    Publish-Telemetry -IsoTs $t0 -Value 10.1
    Write-Host "Published while central up: $t0"

    Write-Host "Stopping central..."
    Stop-Process -Id $central.Id -Force
    $central = $null
    Start-Sleep -Seconds 1

    Write-Host "Publishing while central is down (QoS1, durable session)..."
    for ($i = 1; $i -le $DownPublishCount; $i++) {
        $ts = (Get-Date).ToUniversalTime().AddSeconds($i).ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
        Publish-Telemetry -IsoTs $ts -Value (20 + $i)
        Write-Host " - down msg $i ts=$ts"
    }

    Write-Host "Starting central (phase 2)..."
    $central = Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $logOut -RedirectStandardError $logErr -ArgumentList @("run","-p","central-server")
    Wait-CentralReady -ErrLog $logErr
    Start-Sleep -Seconds 4

    $sql = @"
SELECT tag_code, ts, received_at, payload_json->>'timestamp' AS payload_ts
FROM telemetry_ingest_events
WHERE site_code = '$Site' AND edge_code = '$Agent' AND tag_code = '$Tag'
ORDER BY id DESC
LIMIT 20;
"@
    Write-Host ""
    Write-Host "===== Telemetry Replay Verification ====="
    psql "$PgDsn" -v ON_ERROR_STOP=1 -c $sql

    $countSql = "SELECT COUNT(*)::bigint FROM telemetry_ingest_events WHERE site_code = '$Site' AND edge_code = '$Agent' AND tag_code = '$Tag';"
    $count = (psql "$PgDsn" -t -A -c $countSql).Trim()
    Write-Host "Rows for $Tag: $count"
    Write-Host "Expected at least: $([int]$DownPublishCount + 1)"
}
finally {
    if ($null -ne $central -and -not $central.HasExited) {
        try { Stop-Process -Id $central.Id -Force } catch {}
    }
    Write-Host "Logs: $logOut / $logErr"
}
