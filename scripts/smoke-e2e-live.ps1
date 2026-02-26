param(
    [string]$UiPort = "3015",
    [string]$ComPort = "COM7",
    [int]$AutoStopSeconds = 35,
    [int]$StartupTimeoutSeconds = 120
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

function Wait-HttpOk([string]$Url, [int]$TimeoutSeconds) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        try {
            $res = Invoke-WebRequest -Uri $Url -Method Get -UseBasicParsing -TimeoutSec 3
            if ($res.StatusCode -ge 200 -and $res.StatusCode -lt 500) {
                return
            }
        }
        catch {}
        Start-Sleep -Milliseconds 800
    }
    throw "Timeout waiting for $Url"
}

function Get-TelemetryCount([string]$Dsn) {
    $sql = "SELECT COUNT(*) FROM telemetry_ingest_events;"
    $out = psql "$Dsn" -At -c "$sql"
    if ($LASTEXITCODE -ne 0) {
        throw "psql count query failed"
    }
    return [int64]$out.Trim()
}

Require-Command "powershell"
Require-Command "psql"

$runner = "scripts/dev-run-all-local.ps1"
if (-not (Test-Path $runner)) {
    throw "Missing script: $runner"
}

$pgDsn = $env:CENTRAL_PG_DSN
if ([string]::IsNullOrWhiteSpace($pgDsn)) {
    $pgDsn = "host=127.0.0.1 port=55432 user=postgres dbname=rustscada sslmode=disable"
}

$logDir = "data/e2e"
if (-not (Test-Path $logDir)) { New-Item -ItemType Directory -Path $logDir | Out-Null }
$smokeOut = Join-Path $logDir "smoke-e2e-live.out.log"
$smokeErr = Join-Path $logDir "smoke-e2e-live.err.log"
Remove-Item -ErrorAction SilentlyContinue $smokeOut, $smokeErr

$proc = $null
$ok = $false
try {
    Write-Host "Starting integrated dev stack in background..."
    $proc = Start-Process powershell -PassThru -NoNewWindow -RedirectStandardOutput $smokeOut -RedirectStandardError $smokeErr -ArgumentList @(
        "-ExecutionPolicy","Bypass",
        "-File",$runner,
        "-ComPort",$ComPort,
        "-UiPort",$UiPort,
        "-AutoStopSeconds","$AutoStopSeconds"
    )

    Wait-HttpOk -Url "http://127.0.0.1:8088/health/live" -TimeoutSeconds $StartupTimeoutSeconds
    Wait-HttpOk -Url ("http://127.0.0.1:" + $UiPort + "/live") -TimeoutSeconds $StartupTimeoutSeconds
    Write-Host "Health OK: API and HMI are up."

    $tags = Invoke-RestMethod -Uri "http://127.0.0.1:8088/api/tags/current?limit=20" -Method Get -TimeoutSec 10
    if ($null -eq $tags -or $tags.Count -lt 1) {
        throw "No tags returned by /api/tags/current"
    }
    Write-Host ("Tags current OK: {0} row(s)." -f $tags.Count)

    $countA = Get-TelemetryCount -Dsn $pgDsn
    Start-Sleep -Seconds 4
    $countB = Get-TelemetryCount -Dsn $pgDsn
    if ($countB -le $countA) {
        throw "Telemetry count did not increase ($countA -> $countB)"
    }
    Write-Host ("Telemetry growth OK: {0} -> {1}" -f $countA, $countB)

    Wait-Process -Id $proc.Id -Timeout ($AutoStopSeconds + 45)

    $verifyScript = Join-Path $PSScriptRoot "verify-stop-ports.ps1"
    if (-not (Test-Path $verifyScript)) {
        throw "Missing script: $verifyScript"
    }
    & $verifyScript -Ports @("8088", "$UiPort") -TimeoutSeconds 20
    if ($LASTEXITCODE -ne 0) {
        throw "Port release verification failed"
    }
    Write-Host "Shutdown OK: API/UI ports released."

    $ok = $true
}
finally {
    if ($null -ne $proc -and -not $proc.HasExited) {
        try { taskkill /PID $proc.Id /T /F | Out-Null } catch {}
    }
    if (-not $ok) {
        Write-Host ""
        Write-Host "===== SMOKE OUT (tail) ====="
        if (Test-Path $smokeOut) { Get-Content $smokeOut | Select-Object -Last 60 }
        Write-Host ""
        Write-Host "===== SMOKE ERR (tail) ====="
        if (Test-Path $smokeErr) { Get-Content $smokeErr | Select-Object -Last 60 }
    }
}

if (-not $ok) {
    throw "Smoke E2E failed."
}

Write-Host ""
Write-Host "Smoke E2E PASSED."
