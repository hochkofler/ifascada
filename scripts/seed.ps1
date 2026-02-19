# Seed the database with test data
# Usage: ./scripts/seed.ps1 [seed_file_name]
# Default: seed_test_rs232_full_scale.sql

param (
    [string]$SeedFile = "seed_test_rs232_full_scale.sql"
)

$ErrorActionPreference = "Stop"

# Load environment variables
if (Test-Path .env) {
    Get-Content .env | ForEach-Object {
        if ($_ -match '^([^=]+)=(.*)$') {
            $key = $matches[1]
            $value = $matches[2]
            [Environment]::SetEnvironmentVariable($key, $value, "Process")
        }
    }
}

$DATABASE_URL = $env:DATABASE_URL

if (-not $DATABASE_URL) {
    Write-Host "ERROR: DATABASE_URL not set" -ForegroundColor Red
    exit 1
}

# Parse connection string
# Format: postgres://user:password@host:port/database
# Host and port are parsed together by ([^/]+) if not careful
if ($DATABASE_URL -match 'postgres://([^:]+):([^@]+)@([^/:]+)(?::(\d+))?/(.+)') {
    $db_user = $matches[1]
    $db_password = $matches[2]
    $db_host = $matches[3]
    $db_port = if ($matches[4]) { $matches[4] } else { "5432" }
    $db_name = $matches[5]
}
else {
    Write-Host "ERROR: Invalid DATABASE_URL format" -ForegroundColor Red
    exit 1
}

$FilePath = "scripts/$SeedFile"
if (-not (Test-Path $FilePath)) {
    Write-Host "ERROR: Seed file not found: $FilePath" -ForegroundColor Red
    exit 1
}

Write-Host "Seeding database $db_name on $db_host..." -ForegroundColor Cyan

# Set password for psql
$env:PGPASSWORD = $db_password

# Apply seed
psql -U $db_user -h $db_host -p $db_port -d $db_name -f $FilePath

if ($LASTEXITCODE -eq 0) {
    Write-Host "Seed applied successfully" -ForegroundColor Green
}
else {
    Write-Host "Seed failed" -ForegroundColor Red
    exit 1
}

# Remove password from environment
Remove-Item Env:\PGPASSWORD
