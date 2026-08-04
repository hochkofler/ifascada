param(
    [string]$ServiceName = "ifascada-edge",
    [string]$TaskName = "ifascada-edge",
    [string]$InstallRoot = "C:\\Program Files\\ifascada\\edge",
    [string]$DataRoot = "C:\\ProgramData\\ifascada\\edge",
    [switch]$RemoveData
)

$ErrorActionPreference = "Stop"

$svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($null -ne $svc) {
    if ($svc.Status -ne "Stopped") {
        Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 1
    }
    sc.exe delete $ServiceName | Out-Null
}

$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($null -ne $task) {
    Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
}

Get-Process -Name "edge-agent" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$runnerProcesses = Get-CimInstance Win32_Process -Filter "Name = 'powershell.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -like "*run-edge.ps1*" }
foreach ($proc in $runnerProcesses) {
    Stop-Process -Id $proc.ProcessId -Force -ErrorAction SilentlyContinue
}

if (Test-Path $InstallRoot) {
    Remove-Item $InstallRoot -Recurse -Force -ErrorAction SilentlyContinue
}

if ($RemoveData -and (Test-Path $DataRoot)) {
    Remove-Item $DataRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Uninstalled service: $ServiceName"
if ($RemoveData) {
    Write-Host "Data removed: $DataRoot"
}
