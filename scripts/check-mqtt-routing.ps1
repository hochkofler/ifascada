param(
    [string]$CentralEnv = ".env.central",
    [string[]]$EdgeEnvs = @(
        ".env.edge-com-01",
        ".env.edge-modbus-01",
        ".env.edge-01"
    ),
    [string]$MosquittoContainer = "ifascada-mosquitto",
    [int]$ListenSeconds = 8
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Parse-EnvFile {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        return @{}
    }
    $map = @{}
    Get-Content $Path | ForEach-Object {
        $line = $_.Trim()
        if ([string]::IsNullOrWhiteSpace($line)) { return }
        if ($line.StartsWith("#")) { return }
        $idx = $line.IndexOf("=")
        if ($idx -lt 1) { return }
        $k = $line.Substring(0, $idx).Trim()
        $v = $line.Substring($idx + 1).Trim()
        $map[$k] = $v
    }
    return $map
}

function Get-MqttConfig {
    param([hashtable]$EnvMap)
    $mqttHost = $EnvMap["MQTT_HOST"]
    $mqttPort = $EnvMap["MQTT_PORT"]
    if (-not $mqttHost) { $mqttHost = "127.0.0.1" }
    if (-not $mqttPort) { $mqttPort = "1883" }
    return [PSCustomObject]@{
        Host = $mqttHost
        Port = [int]$mqttPort
    }
}

Write-Host "=== MQTT Routing Check ==="
Write-Host "Date: $(Get-Date -Format o)"
Write-Host ""

$centralMap = Parse-EnvFile -Path $CentralEnv
$central = Get-MqttConfig -EnvMap $centralMap

Write-Host "[Central]"
Write-Host "  env:  $CentralEnv"
Write-Host "  host: $($central.Host)"
Write-Host "  port: $($central.Port)"
Write-Host ""

$rows = @()
foreach ($edgeEnv in $EdgeEnvs) {
    $envMap = Parse-EnvFile -Path $edgeEnv
    if ($envMap.Count -eq 0) { continue }
    $mqtt = Get-MqttConfig -EnvMap $envMap
    $edgeId = $envMap["EDGE_AGENT"]
    if (-not $edgeId) { $edgeId = "<unset>" }
    $rows += [PSCustomObject]@{
        EnvFile = $edgeEnv
        EdgeAgent = $edgeId
        MqttHost = $mqtt.Host
        MqttPort = $mqtt.Port
        MatchesCentral = (($mqtt.Host -eq $central.Host) -and ($mqtt.Port -eq $central.Port))
    }
}

if ($rows.Count -eq 0) {
    Write-Host "No edge env files found with MQTT config."
} else {
    Write-Host "[Edge Env Comparison]"
    $rows | Format-Table -AutoSize
}

Write-Host ""
Write-Host "[TCP Reachability]"
$targets = @(
    [PSCustomObject]@{ Name = "central"; Host = $central.Host; Port = $central.Port }
)
foreach ($r in $rows) {
    $targets += [PSCustomObject]@{
        Name = "edge:$($r.EdgeAgent)"
        Host = $r.MqttHost
        Port = $r.MqttPort
    }
}

$targets = $targets | Sort-Object Host, Port -Unique
foreach ($t in $targets) {
    $ok = $false
    try {
        $res = Test-NetConnection $t.Host -Port $t.Port -WarningAction SilentlyContinue
        $ok = [bool]$res.TcpTestSucceeded
    } catch {
        $ok = $false
    }
    $mark = if ($ok) { "OK" } else { "FAIL" }
    Write-Host ("  {0,-20} {1}:{2} => {3}" -f $t.Name, $t.Host, $t.Port, $mark)
}

Write-Host ""
Write-Host "[Docker Mosquitto Telemetry Sniff]"
try {
    $status = docker inspect -f "{{.State.Running}}" $MosquittoContainer 2>$null
    if ($LASTEXITCODE -ne 0 -or $status.Trim() -ne "true") {
        Write-Host "  Container '$MosquittoContainer' not running. Skipping sniff."
    } else {
        Write-Host "  Listening $ListenSeconds s on topic: scada/plant-a/edge/+/telemetry/tag/#"
        $cmd = "timeout ${ListenSeconds}s mosquitto_sub -h localhost -p 1883 -t 'scada/plant-a/edge/+/telemetry/tag/#' -v"
        docker exec $MosquittoContainer sh -lc $cmd
    }
} catch {
    Write-Host "  Failed to sniff docker mosquitto: $($_.Exception.Message)"
}

Write-Host ""
Write-Host "Done."
