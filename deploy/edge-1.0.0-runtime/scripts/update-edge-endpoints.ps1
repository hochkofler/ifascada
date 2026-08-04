param(
    [string]$CentralHost,
    [int]$MqttPort = 51883,
    [int]$CentralApiPort = 8088,
    [string]$ServiceName = "ifascada-edge",
    [string]$TaskName = "ifascada-edge",
    [string]$EnvFile = "C:\\ProgramData\\ifascada\\edge\\edge.env"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($CentralHost)) {
    throw "Pass -CentralHost with the new central IP/host."
}
if (-not (Test-Path $EnvFile)) {
    throw "Missing env file: $EnvFile"
}

$lines = Get-Content $EnvFile
$map = @{}
foreach ($line in $lines) {
    if ($line -match '^\s*#' -or $line -notmatch '=') { continue }
    $parts = $line -split '=', 2
    $map[$parts[0].Trim()] = $parts[1]
}

$map["MQTT_HOST"] = $CentralHost
$map["MQTT_PORT"] = "$MqttPort"
$map["EDGE_CONFIG_URL"] = "http://${CentralHost}:$CentralApiPort"

$ordered = @()
foreach ($k in $map.Keys | Sort-Object) {
    $ordered += "$k=$($map[$k])"
}
$ordered | Set-Content $EnvFile -Encoding ASCII

$svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($null -ne $svc) {
    if ($svc.Status -ne "Stopped") {
        Restart-Service -Name $ServiceName -Force
    } else {
        Start-Service -Name $ServiceName
    }
    Write-Host "Updated endpoint config and restarted service: $ServiceName"
    exit 0
}

$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($null -ne $task) {
    Stop-Process -Name "edge-agent" -Force -ErrorAction SilentlyContinue
    Start-ScheduledTask -TaskName $TaskName
    Write-Host "Updated endpoint config and restarted scheduled task: $TaskName"
    exit 0
}

throw "No edge runner found. Neither service '$ServiceName' nor task '$TaskName' exists."
