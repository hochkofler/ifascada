param(
    [string]$Site = "plant-a",
    [string]$Agent = "edge-01",
    [string]$MqttHost = "127.0.0.1",
    [int]$MqttPort = 1883,
    [string]$ReadComPort = "COM7",
    [string]$WriteComPort = "COM9",
    [string]$BootstrapPath = "crates/edge-agent/config/bootstrap.serial-scale.example.json",
    [int]$WarmupSeconds = 5,
    [int]$CaptureSeconds = 30
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

function Ensure-Path([string]$Path) {
    if (Test-Path $Path) { return }
    throw "File not found: $Path"
}

function New-LogPath([string]$Name) {
    $dir = "data/e2e"
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    return Join-Path $dir $Name
}

Require-Command "cargo"
Require-Command "mosquitto_sub"
Ensure-Path $BootstrapPath
Ensure-Path "scripts/mock-scale-com.ps1"

$telemetryCompoundTopic = "scada/$Site/edge/$Agent/telemetry/tag/tag_scale_compound"
$telemetryRawTopic = "scada/$Site/edge/$Agent/telemetry/tag/tag_scale_raw"

$logTelemetryCompound = New-LogPath "serial-mock-telemetry-compound.log"
$logTelemetryRaw = New-LogPath "serial-mock-telemetry-raw.log"
$logEdge = New-LogPath "serial-mock-edge-agent.log"
$logEdgeErr = New-LogPath "serial-mock-edge-agent.err.log"
$logMock = New-LogPath "serial-mock-writer.log"
$tempBootstrap = New-LogPath "bootstrap.serial-scale.mock.runtime.json"

Remove-Item -ErrorAction SilentlyContinue $logTelemetryCompound, $logTelemetryRaw, $logEdge, $logEdgeErr, $logMock, $tempBootstrap

$bootstrapRaw = Get-Content $BootstrapPath -Raw
$bootstrapRaw = $bootstrapRaw -replace '"port"\s*:\s*"COM\d+"', ('"port": "' + $ReadComPort + '"')
[System.IO.File]::WriteAllText($tempBootstrap, $bootstrapRaw, (New-Object System.Text.UTF8Encoding($false)))

$procs = @()
try {
    Write-Host "Subscribing telemetry topics:"
    Write-Host " - $telemetryCompoundTopic"
    Write-Host " - $telemetryRawTopic"
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logTelemetryCompound -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$telemetryCompoundTopic)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logTelemetryRaw -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$telemetryRawTopic)

    Write-Host "Starting edge-agent (read $ReadComPort)..."
    $env:EDGE_MQTT_ENABLED = "true"
    $env:EDGE_SITE = $Site
    $env:EDGE_AGENT = $Agent
    $env:MQTT_HOST = $MqttHost
    $env:MQTT_PORT = "$MqttPort"
    $env:EDGE_BOOTSTRAP_PATH = $tempBootstrap
    $procs += Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $logEdge -RedirectStandardError $logEdgeErr -ArgumentList @("run","-p","edge-agent")

    Write-Host "Warmup $WarmupSeconds seconds..."
    Start-Sleep -Seconds $WarmupSeconds

    Write-Host "Starting mock scale writer (write $WriteComPort)..."
    $procs += Start-Process powershell -PassThru -NoNewWindow -RedirectStandardOutput $logMock -ArgumentList @(
        "-ExecutionPolicy","Bypass",
        "-File","scripts/mock-scale-com.ps1",
        "-Port",$WriteComPort,
        "-Count","30",
        "-IntervalMs","500"
    )

    Write-Host "Capturing for $CaptureSeconds seconds..."
    Start-Sleep -Seconds $CaptureSeconds

    Write-Host ""
    Write-Host "===== MOCK WRITER ====="
    if (Test-Path $logMock) { Get-Content $logMock | Select-Object -Last 40 }
    Write-Host ""
    Write-Host "===== TELEMETRY RAW ====="
    if (Test-Path $logTelemetryRaw) { Get-Content $logTelemetryRaw | Select-Object -Last 30 }
    Write-Host ""
    Write-Host "===== TELEMETRY COMPOUND ====="
    if (Test-Path $logTelemetryCompound) { Get-Content $logTelemetryCompound | Select-Object -Last 30 }
    Write-Host ""
    Write-Host "===== EDGE-AGENT ERR (tail) ====="
    if (Test-Path $logEdgeErr) { Get-Content $logEdgeErr | Select-Object -Last 40 }
    Write-Host ""
    Write-Host "Logs saved in data/e2e/"
}
finally {
    Write-Host "Stopping spawned processes..."
    foreach ($p in $procs) {
        if ($null -ne $p -and -not $p.HasExited) {
            try { Stop-Process -Id $p.Id -Force } catch {}
        }
    }
}
