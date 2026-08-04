param(
    [ValidateSet("minimal", "sim20", "full")]
    [string]$SeedProfile = "minimal",
    [switch]$SkipSeed,
    [switch]$WithImageLoad,
    [switch]$WithWebUi
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

if ($WithImageLoad -and (Test-Path ".\images\central-server-1.0.0.tar")) {
    docker load -i .\images\central-server-1.0.0.tar
    if ($LASTEXITCODE -ne 0) { throw "docker load failed" }
}
if ($WithImageLoad -and (Test-Path ".\images\web-ui-1.0.0.tar")) {
    docker load -i .\images\web-ui-1.0.0.tar
    if ($LASTEXITCODE -ne 0) { throw "docker load web-ui failed" }
}

docker compose -f .\docker-compose.yml up -d timescaledb redis mosquitto
if ($LASTEXITCODE -ne 0) { throw "infra up failed" }

if (-not $SkipSeed) {
    $env:SEED_PROFILE = $SeedProfile
    docker compose -f .\docker-compose.yml --profile seed up --abort-on-container-exit --exit-code-from db-seed db-seed
    if ($LASTEXITCODE -ne 0) { throw "db seed failed" }
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    docker compose -f .\docker-compose.yml rm -f db-seed | Out-Null
    $ErrorActionPreference = $prev
    Remove-Item Env:SEED_PROFILE -ErrorAction SilentlyContinue
}

docker compose -f .\docker-compose.yml --profile central up -d central-server
if ($LASTEXITCODE -ne 0) { throw "central up failed" }

if ($WithWebUi) {
    docker compose -f .\docker-compose.yml --profile central --profile webui up -d central-server web-ui
    if ($LASTEXITCODE -ne 0) { throw "web-ui up failed" }
}

Write-Host "Done"
Write-Host "Health: http://127.0.0.1:8088/health/live"
if ($WithWebUi) {
    Write-Host "Web UI: http://127.0.0.1:3001"
}
