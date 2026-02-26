param(
    [string]$PgDsn = $env:CENTRAL_PG_DSN,
    [ValidateSet("minimal", "sim20", "full")]
    [string]$SeedProfile = "minimal",
    [switch]$ResetSchema
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

function Invoke-PsqlFile([string]$Dsn, [string]$File) {
    if (-not (Test-Path $File)) {
        throw "Missing SQL file: $File"
    }
    Write-Host "Applying $File ..."
    psql "$Dsn" -v ON_ERROR_STOP=1 -f "$File"
    if ($LASTEXITCODE -ne 0) {
        throw "psql failed on $File (exit code $LASTEXITCODE)"
    }
}

function Test-TimescaleAvailable([string]$Dsn) {
    $sql = "SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'timescaledb');"
    $result = psql "$Dsn" -t -A -c $sql 2>$null
    if ($LASTEXITCODE -ne 0) { return $false }
    return ($result.Trim().ToLower() -eq "t")
}

if ([string]::IsNullOrWhiteSpace($PgDsn)) {
    throw "PgDsn is empty. Pass -PgDsn or set CENTRAL_PG_DSN."
}

Require-Command "psql"

if ($ResetSchema) {
    Write-Host "Resetting schema public ..."
    psql "$PgDsn" -v ON_ERROR_STOP=1 -c "DROP SCHEMA IF EXISTS public CASCADE; CREATE SCHEMA public;"
    if ($LASTEXITCODE -ne 0) {
        throw "Failed resetting schema public"
    }
}

$baseMigrations = @(
    "crates/central-server/migrations/0001_core_postgres.sql",
    "crates/central-server/migrations/0003_tag_naming_governance.sql",
    "crates/central-server/migrations/0005_fix_tag_naming_constraint_regex.sql",
    "crates/central-server/migrations/0006_context_hierarchy.sql",
    "crates/central-server/migrations/0009_operational_events.sql",
    "crates/central-server/migrations/0010_connection_domain_state.sql",
    "crates/central-server/migrations/0011_device_domain_state.sql",
    "crates/central-server/migrations/0012_edges_metadata_json.sql",
    "crates/central-server/migrations/0016_telemetry_received_at.sql"
)

$seedByProfile = @{
    "minimal" = @(
        "crates/central-server/migrations/0015_dev_seed_minimal_three_edges.sql",
        "crates/central-server/migrations/0017_printer_device_command_and_negative_trigger.sql"
    )
    "sim20" = @(
        "crates/central-server/migrations/0004_dev_seed_minimal_catalog.sql",
        "crates/central-server/migrations/0007_dev_seed_context_hierarchy.sql",
        "crates/central-server/migrations/0008_dev_seed_sim20_multi_area.sql"
    )
    "full" = @(
        "crates/central-server/migrations/0004_dev_seed_minimal_catalog.sql",
        "crates/central-server/migrations/0007_dev_seed_context_hierarchy.sql",
        "crates/central-server/migrations/0008_dev_seed_sim20_multi_area.sql",
        "crates/central-server/migrations/0013_scale_manual_config_in_catalog.sql",
        "crates/central-server/migrations/0014_dev_seed_modbus_rtu_com10_multi_slave.sql",
        "crates/central-server/migrations/0017_printer_device_command_and_negative_trigger.sql"
    )
}

foreach ($m in $baseMigrations) {
    Invoke-PsqlFile -Dsn $PgDsn -File $m
}

$timescaleMigration = "crates/central-server/migrations/0002_timescale_historian.sql"
if (Test-TimescaleAvailable -Dsn $PgDsn) {
    Invoke-PsqlFile -Dsn $PgDsn -File $timescaleMigration
} else {
    Write-Host "Skipping $timescaleMigration (timescaledb extension not available on target PostgreSQL)."
}

foreach ($s in $seedByProfile[$SeedProfile]) {
    Invoke-PsqlFile -Dsn $PgDsn -File $s
}

Write-Host ""
Write-Host "Done."
Write-Host "Seed profile: $SeedProfile"
Write-Host "Reset schema: $ResetSchema"
