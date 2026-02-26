param(
    [switch]$IncludeEdge,
    [switch]$IncludeApplication,
    [switch]$ForceStopRunning
)

$ErrorActionPreference = "Stop"

function Run-Step([string]$Name, [string]$Cmd) {
    Write-Host ""
    Write-Host "==> $Name"
    Write-Host "    $Cmd"
    Invoke-Expression $Cmd
    if ($LASTEXITCODE -ne 0) {
        throw "Step failed: $Name"
    }
}

function Ensure-BinaryFree([string[]]$ProcessNames, [switch]$ForceStop) {
    $running = Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $ProcessNames -contains $_.ProcessName
    }
    if (-not $running) { return }

    if ($ForceStop) {
        foreach ($p in $running) {
            try { Stop-Process -Id $p.Id -Force } catch {}
        }
        Start-Sleep -Milliseconds 300
        return
    }

    $names = ($running | Select-Object -ExpandProperty ProcessName -Unique) -join ", "
    throw "Running processes block test rebuild: $names. Stop them or re-run with -ForceStopRunning."
}

Ensure-BinaryFree -ProcessNames @("central-server","edge-agent") -ForceStop:$ForceStopRunning

Run-Step "Central ingestion flow contract" "cargo test -q -p central-server --test ingestion_flow_tests"
Run-Step "Central SSE contract" "cargo test -q -p central-server --test api_sse_contract_tests"
Run-Step "Central edges/connections contract" "cargo test -q -p central-server --test api_connections_contract_tests"
Run-Step "Central edge config governance contract" "cargo test -q -p central-server --test api_edge_config_contract_tests"
Run-Step "Central device status contract" "cargo test -q -p central-server --test api_device_status_contract_tests"
Run-Step "Central runtime heartbeat contract" "cargo test -q -p central-server --test api_runtime_status_heartbeat_contract_tests"
Run-Step "Central tag status contract" "cargo test -q -p central-server --test api_tag_status_contract_tests"

if ($IncludeApplication) {
    Run-Step "Application runtime tests" "cargo test -q -p application"
}

if ($IncludeEdge) {
    Run-Step "Edge agent tests" "cargo test -q -p edge-agent"
}

Write-Host ""
Write-Host "Baseline contracts: OK"
