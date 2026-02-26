param(
    [string]$Site = "plant-a",
    [string]$Agent = "edge-01",
    [string]$MqttHost = "127.0.0.1",
    [int]$MqttPort = 1883,
    [string]$ComPort = "COM7",
    [string]$BootstrapPath = "crates/edge-agent/config/bootstrap.serial-scale.example.json",
    [int]$WarmupSeconds = 5,
    [int]$CaptureSeconds = 20
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

$telemetryCompoundTopic = "scada/$Site/edge/$Agent/telemetry/tag/tag_scale_compound"
$telemetryRawTopic = "scada/$Site/edge/$Agent/telemetry/tag/tag_scale_raw"
$healthTopic = "scada/$Site/edge/$Agent/health/runtime"
$alertsTopic = "scada/$Site/edge/$Agent/alerts/runtime"

$logTelemetryCompound = New-LogPath "serial-telemetry-compound.log"
$logTelemetryRaw = New-LogPath "serial-telemetry-raw.log"
$logHealth = New-LogPath "serial-health.log"
$logAlerts = New-LogPath "serial-alerts.log"
$logEdge = New-LogPath "serial-edge-agent.log"
$logEdgeErr = New-LogPath "serial-edge-agent.err.log"
$tempBootstrap = New-LogPath "bootstrap.serial-scale.runtime.json"

Remove-Item -ErrorAction SilentlyContinue $logTelemetryCompound, $logTelemetryRaw, $logHealth, $logAlerts, $logEdge, $logEdgeErr, $tempBootstrap

$bootstrapRaw = Get-Content $BootstrapPath -Raw
$bootstrapRaw = $bootstrapRaw -replace '"port"\s*:\s*"COM\d+"', ('"port": "' + $ComPort + '"')
[System.IO.File]::WriteAllText($tempBootstrap, $bootstrapRaw, (New-Object System.Text.UTF8Encoding($false)))

$procs = @()
try {
    Write-Host "Subscribing telemetry topics:"
    Write-Host " - $telemetryCompoundTopic"
    Write-Host " - $telemetryRawTopic"
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logTelemetryCompound -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$telemetryCompoundTopic)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logTelemetryRaw -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$telemetryRawTopic)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logHealth -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$healthTopic)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logAlerts -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$alertsTopic)

    Write-Host "Starting edge-agent with serial bootstrap on $ComPort ..."
    $env:EDGE_MQTT_ENABLED = "true"
    $env:EDGE_SITE = $Site
    $env:EDGE_AGENT = $Agent
    $env:MQTT_HOST = $MqttHost
    $env:MQTT_PORT = "$MqttPort"
    $env:EDGE_BOOTSTRAP_PATH = $tempBootstrap
    $procs += Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $logEdge -RedirectStandardError $logEdgeErr -ArgumentList @("run","-p","edge-agent")

    Write-Host "Warmup $WarmupSeconds seconds..."
    Start-Sleep -Seconds $WarmupSeconds

    Write-Host "Capturing scale frames for $CaptureSeconds seconds..."
    Write-Host "Now trigger PRINT on your scale simulator connected to $ComPort."
    Start-Sleep -Seconds $CaptureSeconds

    Write-Host ""
    Write-Host "===== TELEMETRY RAW ====="
    if (Test-Path $logTelemetryRaw) { Get-Content $logTelemetryRaw | Select-Object -Last 30 }
    Write-Host ""
    Write-Host "===== TELEMETRY COMPOUND ====="
    if (Test-Path $logTelemetryCompound) { Get-Content $logTelemetryCompound | Select-Object -Last 30 }
    Write-Host ""
    Write-Host "===== HEALTH ====="
    if (Test-Path $logHealth) { Get-Content $logHealth | Select-Object -Last 10 }
    Write-Host ""
    Write-Host "===== ALERTS ====="
    if (Test-Path $logAlerts) { Get-Content $logAlerts | Select-Object -Last 10 }
    Write-Host ""
    Write-Host "===== EDGE-AGENT LOG (tail) ====="
    if (Test-Path $logEdge) { Get-Content $logEdge | Select-Object -Last 40 }
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
