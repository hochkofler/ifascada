# Apply database migrations for IFA SCADA
# PostgreSQL 18

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
    Write-Host "Please create .env file from .env.example" -ForegroundColor Yellow
    exit 1
}

# Parse connection string
# Format: postgres://user:password@host:port/database
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

Write-Host "Applying migrations to: $db_name on $db_host..." -ForegroundColor Cyan

# Set password for psql
$env:PGPASSWORD = $db_password

# Apply migrations by running the migration script or listing files
# Note: For now we just run the first one as in the original script, 
# but a real migration runner would be better.
psql -U $db_user -h $db_host -p $db_port -d $db_name -f migrations/20260210_001_create_tags_schema.sql

if ($LASTEXITCODE -eq 0) {
    Write-Host "Migration applied successfully" -ForegroundColor Green
}
else {
    Write-Host "Migration failed" -ForegroundColor Red
    exit 1
}

# Remove password from environment
Remove-Item Env:\PGPASSWORD
