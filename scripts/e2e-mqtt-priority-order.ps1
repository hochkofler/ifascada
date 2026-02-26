param(
    [string]$Site = "plant-a",
    [string]$Agent = "edge-01",
    [string]$MqttHost = "127.0.0.1",
    [int]$MqttPort = 1883,
    [string]$BootstrapPath = "crates/edge-agent/config/bootstrap.example.json",
    [int]$WarmupSeconds = 4,
    [int]$CaptureSeconds = 6
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
Require-Command "mosquitto_pub"
Ensure-Path $BootstrapPath

$topicCmd = "scada/$Site/edge/$Agent/cmd/write"
$topicAudit = "scada/$Site/edge/$Agent/audit/write"
$topicAck = "scada/$Site/edge/$Agent/cmd/write/ack"

$logAudit = New-LogPath "priority-audit.log"
$logAck = New-LogPath "priority-ack.log"
$logEdge = New-LogPath "priority-edge-agent.log"
$logEdgeErr = New-LogPath "priority-edge-agent.err.log"
$payloadNormal1 = New-LogPath "priority-cmd-normal-1.json"
$payloadHigh = New-LogPath "priority-cmd-high.json"
$payloadNormal2 = New-LogPath "priority-cmd-normal-2.json"

Remove-Item -ErrorAction SilentlyContinue $logAudit, $logAck, $logEdge, $logEdgeErr, $payloadNormal1, $payloadHigh, $payloadNormal2

$procs = @()
try {
    Write-Host "Starting subscribers..."
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logAudit -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$topicAudit)
    $procs += Start-Process mosquitto_sub -PassThru -NoNewWindow -RedirectStandardOutput $logAck -ArgumentList @("-h",$MqttHost,"-p",$MqttPort,"-v","-t",$topicAck)

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

    $cmdNormal1 = '{"schema_version":1,"source":"manual-e2e","tag_id":"tag_hr_10_cmd","value":101,"command_id":"cmd-normal-1","priority":"normal"}'
    $cmdHigh = '{"schema_version":1,"source":"manual-e2e","tag_id":"tag_hr_11_cmd","value":202,"command_id":"cmd-high-1","priority":"high"}'
    $cmdNormal2 = '{"schema_version":1,"source":"manual-e2e","tag_id":"tag_hr_10_cmd","value":303,"command_id":"cmd-normal-2","priority":"normal"}'

    [System.IO.File]::WriteAllText($payloadNormal1, $cmdNormal1, (New-Object System.Text.UTF8Encoding($false)))
    [System.IO.File]::WriteAllText($payloadHigh, $cmdHigh, (New-Object System.Text.UTF8Encoding($false)))
    [System.IO.File]::WriteAllText($payloadNormal2, $cmdNormal2, (New-Object System.Text.UTF8Encoding($false)))

    Write-Host "Publishing burst: normal -> high -> normal"
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $topicCmd -f $payloadNormal1
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $topicCmd -f $payloadHigh
    & mosquitto_pub -h $MqttHost -p $MqttPort -t $topicCmd -f $payloadNormal2

    Write-Host "Capturing for $CaptureSeconds seconds..."
    Start-Sleep -Seconds $CaptureSeconds

    Write-Host ""
    Write-Host "===== ACK (tail) ====="
    if (Test-Path $logAck) { Get-Content $logAck | Select-Object -Last 20 }

    Write-Host ""
    Write-Host "===== AUDIT (tail) ====="
    if (Test-Path $logAudit) { Get-Content $logAudit | Select-Object -Last 20 }

    Write-Host ""
    Write-Host "===== PRIORITY CHECK ====="
    if (Test-Path $logAudit) {
        $auditLines = Get-Content $logAudit
        $events = @()
        foreach ($ln in $auditLines) {
            $idx = $ln.IndexOf("{")
            if ($idx -lt 0) { continue }
            $json = $ln.Substring($idx)
            try {
                $obj = $json | ConvertFrom-Json
                if ($null -ne $obj.command_id) {
                    $events += [PSCustomObject]@{
                        command_id = [string]$obj.command_id
                        outcome = [string]$obj.outcome
                        tag_id = [string]$obj.tag_id
                    }
                }
            } catch {}
        }

        $first3 = $events | Select-Object -First 3
        if ($first3.Count -gt 0) {
            $first3 | Format-Table -AutoSize | Out-Host
            $order = ($first3 | ForEach-Object { $_.command_id }) -join " -> "
            Write-Host "Observed first command_ids: $order"
            if ($order -like "*cmd-high-1*") {
                Write-Host "High priority command reached audit stream in the early window."
            } else {
                Write-Host "High priority command not visible in first audit window; repeat with higher load if needed."
            }
        } else {
            Write-Host "No audit events parsed. Check edge-agent.err log."
        }
    }

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
