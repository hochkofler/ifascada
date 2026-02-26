param(
    [string]$ComposeFile = "docker-compose.scada.yml",
    [switch]$NoStart,
    [switch]$PersistUserEnv
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

Require-Command "docker"

if (-not (Test-Path $ComposeFile)) {
    throw "Compose file not found: $ComposeFile"
}

if (-not $NoStart) {
    Write-Host "Starting SCADA infrastructure stack from $ComposeFile ..."
    docker compose -f $ComposeFile up -d
}

$vars = @{
    "MQTT_HOST" = "127.0.0.1"
    "MQTT_PORT" = "51883"
    "CENTRAL_MQTT_ENABLED" = "true"
    "CENTRAL_MQTT_CLIENT_ID" = "central-server-01"
    "CENTRAL_MQTT_TOPIC_FILTER" = "scada/+/edge/+/#"
    "CENTRAL_API_ENABLED" = "true"
    "CENTRAL_API_BIND" = "127.0.0.1:8088"
    "CENTRAL_PG_DSN" = "host=127.0.0.1 port=55432 user=postgres dbname=rustscada sslmode=disable"
    "CENTRAL_REDIS_ENABLED" = "true"
    "CENTRAL_REDIS_URL" = "redis://127.0.0.1:56379/"
    "CENTRAL_REDIS_EVENT_CHANNEL" = "scada:rt:events"
    "CENTRAL_REDIS_KEY_TTL_SECS" = "300"
    "EDGE_MQTT_ENABLED" = "true"
    "EDGE_SITE" = "plant-a"
    "EDGE_AGENT" = "edge-01"
}

Write-Host "Applying development environment variables (current session)..."
foreach ($k in $vars.Keys) {
    Set-Item -Path ("Env:{0}" -f $k) -Value $vars[$k]
    Write-Host " - $k=$($vars[$k])"
}

if ($PersistUserEnv) {
    Write-Host "Persisting environment variables to User scope..."
    foreach ($k in $vars.Keys) {
        [Environment]::SetEnvironmentVariable($k, $vars[$k], "User")
    }
    Write-Host "User environment persisted. Open a new terminal to use persisted values."
}

Write-Host ""
Write-Host "Infrastructure status:"
docker compose -f $ComposeFile ps

Write-Host ""
Write-Host "Run central-server locally:"
Write-Host "  cargo run -p central-server"
Write-Host "Run edge-agent locally:"
Write-Host "  cargo run -p edge-agent"
