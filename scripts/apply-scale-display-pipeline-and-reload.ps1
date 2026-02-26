param(
    [string]$PgDsn = $env:CENTRAL_PG_DSN,
    [string]$SqlFile = "scripts/sql-configure-scale-display-pipeline.sql",
    [string]$Site = "plant-a",
    [string]$Edge = "edge-com-01",
    [string]$MqttHost = "127.0.0.1",
    [int]$MqttPort = 51883,
    [string]$MosquittoContainer = "ifascada-mosquitto"
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'."
    }
}

function Publish-ConfigApply([string]$Topic, [string]$Payload) {
    if (Get-Command mosquitto_pub -ErrorAction SilentlyContinue) {
        & mosquitto_pub -h $MqttHost -p $MqttPort -t $Topic -m $Payload
        return
    }

    if (Get-Command docker -ErrorAction SilentlyContinue) {
        $running = docker ps --filter "name=$MosquittoContainer" --format "{{.Names}}"
        if ($running -match $MosquittoContainer) {
            docker exec -i $MosquittoContainer mosquitto_pub -h localhost -p 1883 -t $Topic -m $Payload
            return
        }
    }

    throw "Could not publish config/apply: neither local mosquitto_pub nor running docker container '$MosquittoContainer' is available."
}

if ([string]::IsNullOrWhiteSpace($PgDsn)) {
    throw "PgDsn missing. Set CENTRAL_PG_DSN or pass -PgDsn."
}

if (-not (Test-Path $SqlFile)) {
    throw "SQL file not found: $SqlFile"
}

Require-Command "psql"

Write-Host "Applying SQL pipeline config from '$SqlFile' ..."
psql "$PgDsn" -v ON_ERROR_STOP=1 -f "$SqlFile"
if ($LASTEXITCODE -ne 0) {
    throw "psql failed applying $SqlFile"
}

$topic = "scada/$Site/edge/$Edge/config/apply"
$requestId = "cfg-apply-" + [DateTime]::UtcNow.ToString("yyyyMMddHHmmss")
$payloadObj = @{
    schema_version = 1
    source = "manual"
    request_id = $requestId
}
$payload = $payloadObj | ConvertTo-Json -Compress

Write-Host "Publishing config/apply to '$topic' ..."
Publish-ConfigApply -Topic $topic -Payload $payload

Write-Host ""
Write-Host "Done."
Write-Host " - request_id: $requestId"
Write-Host " - topic:      $topic"
Write-Host ""
Write-Host "Verify pipeline in DB:"
Write-Host "SELECT t.tag_code, t.metadata_json->'pipeline' AS pipeline"
Write-Host "FROM tags t"
Write-Host "JOIN devices d ON d.id = t.device_id"
Write-Host "JOIN connections c ON c.id = d.connection_id"
Write-Host "WHERE c.connection_code='conn_scale_rs232_manual_1' AND t.tag_code='tag_scale_manual_compound';"
