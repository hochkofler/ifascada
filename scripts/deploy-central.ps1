param(
    [string]$ComposeFile = "docker-compose.scada.yml",
    [ValidateSet("minimal", "sim20", "full")]
    [string]$SeedProfile = "minimal",
    [switch]$SkipSeed,
    [switch]$NoCentralStart,
    [switch]$BuildCentral,
    [switch]$WithPgAdmin
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

function Invoke-Checked([scriptblock]$Action, [string]$ErrorMessage) {
    & $Action
    if ($LASTEXITCODE -ne 0) {
        throw "$ErrorMessage (exit code $LASTEXITCODE)"
    }
}

Require-Command "docker"

if (-not (Test-Path $ComposeFile)) {
    throw "Compose file not found: $ComposeFile"
}

$infraServices = @("timescaledb", "redis", "mosquitto")
if ($WithPgAdmin) {
    $infraServices += "pgadmin"
}

Write-Host "Starting central infrastructure from $ComposeFile ..."
Invoke-Checked -ErrorMessage "Failed to start infrastructure services" -Action {
    docker compose -f $ComposeFile up -d @infraServices
}

if (-not $SkipSeed) {
    Write-Host "Applying DB migrations + seed (profile=$SeedProfile) ..."
    $env:SEED_PROFILE = $SeedProfile
    try {
        Invoke-Checked -ErrorMessage "DB seed process failed" -Action {
            docker compose -f $ComposeFile --profile seed up --abort-on-container-exit --exit-code-from db-seed db-seed
        }
    }
    finally {
        docker compose -f $ComposeFile rm -f db-seed *> $null
        Remove-Item Env:SEED_PROFILE -ErrorAction SilentlyContinue
    }
} else {
    Write-Host "Skipping DB seed step (-SkipSeed)."
}

if (-not $NoCentralStart) {
    Write-Host "Starting central-server container ..."
    $upArgs = @("-f", $ComposeFile, "--profile", "central", "up", "-d")
    if ($BuildCentral) {
        $upArgs += "--build"
    }
    $upArgs += "central-server"
    Invoke-Checked -ErrorMessage "Failed to start central-server container" -Action {
        docker compose @upArgs
    }
} else {
    Write-Host "Skipping central-server start (-NoCentralStart)."
}

Write-Host ""
Write-Host "Done."
Write-Host "Infra status:   docker compose -f $ComposeFile ps"
Write-Host "Central logs:   docker logs -f ifascada-central-server"
Write-Host "Central health: http://127.0.0.1:8088/health/live"
