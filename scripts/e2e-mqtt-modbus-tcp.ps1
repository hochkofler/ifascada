param(
    [string]$Site = "plant-a",
    [string]$Agent = "edge-01",
    [string]$MqttHost = "127.0.0.1",
    [int]$MqttPort = 1883,
    [string]$BootstrapPath = "crates/edge-agent/config/bootstrap.example.json",
    [int]$WarmupSeconds = 6,
    [int]$CaptureSeconds = 8
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

function Ensure-Bootstrap([string]$Path) {
    if (Test-Path $Path) { return }
    throw "Bootstrap file not found: $Path"
}

function New-LogPath([string]$Name) {
    $dir = "data/e2e"
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    return Join-Path $dir $Name
}

Require-Command "cargo"
Require-Command "mosquitto_pub"
Require-Command "mosquitto_sub"
Ensure-Bootstrap $BootstrapPath

$topics = @{
    Ack        = "scada/$Site/edge/$Agent/cmd/write/ack"
    Audit      = "scada/$Site/edge/$Agent/audit/write"
    Health     = "scada/$Site/edge/$Agent/health/runtime"
    Alerts     = "scada/$Site/edge/$Agent/alerts/runtime"
    AlertAckRs = "scada/$Site/edge/$Agent/alerts/runtime/ack/result"
}
$cmdTopic = "scada/$Site/edge/$Agent/cmd/write"

$logAck = New-LogPath "ack.log"
$logAudit = New-LogPath "audit.log"
$logHealth = New-LogPath "health.log"
$logAlerts = New-LogPath "alerts.log"
$logAlertAck = New-LogPath "alert-ack-result.log"
$logEdge = New-LogPath "edge-agent.log"
$logEdgeErr = New-LogPath "edge-agent.err.log"
$okPayloadFile = New-LogPath "cmd-ok.json"
$badPayloadFile = New-LogPath "cmd-bad.json"

Remove-Item -ErrorAction SilentlyContinue $logAck, $logAudit, $logHealth, $logAlerts, $logAlertAck, $logEdge, $logEdgeErr, $okPayloadFile, $badPayloadFile

$procs = @()

try {
    Write-Host "Starting MQTT subscribers..."
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logAck -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$topics.Ack)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logAudit -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$topics.Audit)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logHealth -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$topics.Health)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logAlerts -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$topics.Alerts)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logAlertAck -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$topics.AlertAckRs)

    Write-Host "Starting edge-agent..."
    $env:EDGE_MQTT_ENABLED = "true"
    $env:EDGE_SITE = $Site
    $env:EDGE_AGENT = $Agent
    $env:MQTT_HOST = $MqttHost
    $env:MQTT_PORT = "$MqttPort"
    $env:EDGE_BOOTSTRAP_PATH = $BootstrapPath
    $procs += Start-Process cargo -PassThru -NoNewWindow -RedirectStandardOutput $logEdge -RedirectStandardError $logEdgeErr -ArgumentList @("run","-p","edge-agent")

    Write-Host "Warmup $WarmupSeconds seconds..."
    Start-Sleep -Seconds $WarmupSeconds

    $okPayload = '{"schema_version":1,"source":"manual-e2e","tag_id":"tag_hr_10_cmd","value":123,"command_id":"cmd-ok-001"}'
    $badPayload = '{"schema_version":1,"source":"manual-e2e","tag_id":"tag_unknown","value":123,"command_id":"cmd-bad-001"}'
    [System.IO.File]::WriteAllText($okPayloadFile, $okPayload, (New-Object System.Text.UTF8Encoding($false)))
    [System.IO.File]::WriteAllText($badPayloadFile, $badPayload, (New-Object System.Text.UTF8Encoding($false)))

    Write-Host "Publishing command OK..."
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $cmdTopic -f $okPayloadFile

    Write-Host "Publishing command with unknown tag..."
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $cmdTopic -f $badPayloadFile

    Write-Host "Capturing events for $CaptureSeconds seconds..."
    Start-Sleep -Seconds $CaptureSeconds

    Write-Host ""
    Write-Host "===== ACK ====="
    if (Test-Path $logAck) { Get-Content $logAck | Select-Object -Last 30 }
    Write-Host ""
    Write-Host "===== AUDIT ====="
    if (Test-Path $logAudit) { Get-Content $logAudit | Select-Object -Last 30 }
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

