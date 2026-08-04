param(
    [string]$EdgeId = "edge-01",
    [string]$Site = "plant-a",
    [string]$CentralHost = "127.0.0.1",
    [int]$MqttPort = 51883,
    [int]$CentralApiPort = 8088,
    [string]$EnrollToken = "dev-edge-enroll-token",
    [string]$ConfigHmacSecret = "dev-edge-config-signing-secret",
    [string]$ConfigKeyId = "v1",
    [string]$ServiceName = "ifascada-edge",
    [string]$TaskName = "ifascada-edge",
    [ValidateSet("auto", "task", "nssm")]
    [string]$InstallMode = "auto",
    [string]$RunAsUser = "SYSTEM",
    [SecureString]$RunAsPassword,
    [string]$InstallRoot = "C:\\Program Files\\ifascada\\edge",
    [string]$DataRoot = "C:\\ProgramData\\ifascada\\edge"
)

$ErrorActionPreference = "Stop"

function Assert-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Run this script as Administrator."
    }
}

function Remove-ServiceIfExists([string]$Name) {
    $svc = Get-Service -Name $Name -ErrorAction SilentlyContinue
    if ($null -eq $svc) { return }
    if ($svc.Status -ne "Stopped") {
        Stop-Service -Name $Name -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 1
    }
    sc.exe delete $Name | Out-Null
    Start-Sleep -Seconds 1
}

function Remove-TaskIfExists([string]$Name) {
    $task = Get-ScheduledTask -TaskName $Name -ErrorAction SilentlyContinue
    if ($null -ne $task) {
        Unregister-ScheduledTask -TaskName $Name -Confirm:$false
    }
}

function Stop-EdgeRuntime([string]$TaskToStop) {
    Stop-ScheduledTask -TaskName $TaskToStop -ErrorAction SilentlyContinue
    Get-Process -Name "edge-agent" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    $runnerProcesses = Get-CimInstance Win32_Process -Filter "Name = 'powershell.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -like "*run-edge.ps1*" }
    foreach ($proc in $runnerProcesses) {
        Stop-Process -Id $proc.ProcessId -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 1
}

function Get-PlainPassword([SecureString]$ProvidedPassword, [string]$UserName) {
    $secure = $ProvidedPassword
    if ($null -eq $secure) {
        $secure = Read-Host "Password for $UserName" -AsSecureString
    }
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try {
        return [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
    }
}

function Install-TaskRunner([string]$Name, [string]$ScriptPath, [string]$UserName, [SecureString]$PasswordValue) {
    Remove-TaskIfExists -Name $Name
    $taskAction = New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptPath`""
    $taskTrigger = New-ScheduledTaskTrigger -AtStartup
    $taskSettings = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Hours 0)
    if ($UserName -eq "SYSTEM") {
        $taskPrincipal = New-ScheduledTaskPrincipal -UserId "SYSTEM" -RunLevel Highest -LogonType ServiceAccount
        Register-ScheduledTask -TaskName $Name -Action $taskAction -Trigger $taskTrigger -Principal $taskPrincipal -Settings $taskSettings | Out-Null
    } else {
        $plain = Get-PlainPassword -ProvidedPassword $PasswordValue -UserName $UserName
        Register-ScheduledTask -TaskName $Name -Action $taskAction -Trigger $taskTrigger -User $UserName -Password $plain -RunLevel Highest -Settings $taskSettings | Out-Null
    }
    Start-ScheduledTask -TaskName $Name
}

function Install-NssmService([string]$Name, [string]$ScriptPath, [string]$WorkingDir, [string]$OutLog, [string]$ErrLog) {
    $nssm = Get-Command nssm -ErrorAction SilentlyContinue
    if ($null -eq $nssm) {
        throw "InstallMode=nssm but nssm is not installed."
    }
    Remove-ServiceIfExists -Name $Name
    $psExe = Join-Path $env:SystemRoot "System32\\WindowsPowerShell\\v1.0\\powershell.exe"
    & $nssm.Source install $Name $psExe "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptPath`""
    & $nssm.Source set $Name AppDirectory $WorkingDir
    & $nssm.Source set $Name AppStdout $OutLog
    & $nssm.Source set $Name AppStderr $ErrLog
    & $nssm.Source set $Name Start SERVICE_AUTO_START
    & $nssm.Source start $Name
}

Assert-Admin

$pkgRoot = Split-Path -Path $PSScriptRoot -Parent
$binSource = Join-Path $pkgRoot "bin\\edge-agent.exe"
$bootstrapSource = Join-Path $pkgRoot "config\\bootstrap.example.json"
$envTemplate = Join-Path $pkgRoot "config\\edge.env.example"

if (-not (Test-Path $binSource)) {
    throw "Missing binary: $binSource"
}
if (-not (Test-Path $envTemplate)) {
    throw "Missing env template: $envTemplate"
}

$logDir = Join-Path $DataRoot "logs"
$configDir = Join-Path $DataRoot "config"
$bootstrapTarget = Join-Path $configDir "bootstrap.json"
$envTarget = Join-Path $DataRoot "edge.env"
$runScript = Join-Path $DataRoot "run-edge.ps1"
$exeTarget = Join-Path $InstallRoot "edge-agent.exe"

New-Item -ItemType Directory -Force -Path $InstallRoot | Out-Null
New-Item -ItemType Directory -Force -Path $DataRoot | Out-Null
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
New-Item -ItemType Directory -Force -Path $configDir | Out-Null

Stop-EdgeRuntime -TaskToStop $TaskName
Copy-Item $binSource $exeTarget -Force
if (Test-Path $bootstrapSource) {
    Copy-Item $bootstrapSource $bootstrapTarget -Force
}

$envLines = @(
    "RUST_LOG=info,edge_agent=debug",
    "EDGE_MQTT_ENABLED=true",
    "EDGE_SITE=$Site",
    "EDGE_AGENT=$EdgeId",
    "MQTT_HOST=$CentralHost",
    "MQTT_PORT=$MqttPort",
    "MQTT_CLIENT_ID=$EdgeId",
    "",
    "EDGE_CONFIG_URL=http://${CentralHost}:$CentralApiPort",
    "EDGE_ENROLL_TOKEN=$EnrollToken",
    "EDGE_CONFIG_HMAC_SECRET=$ConfigHmacSecret",
    "EDGE_CONFIG_KEY_ID=$ConfigKeyId",
    "",
    "EDGE_BOOTSTRAP_PATH=$bootstrapTarget",
    "MQTT_OUTBOX_PATH=$DataRoot\\mqtt_outbox.db",
    "EDGE_RUNTIME_CACHE_PATH=$DataRoot\\runtime_config.signed.json",
    "EDGE_CONFIG_APPLY_RECEIPT_PATH=$DataRoot\\config_apply_receipt.json"
)
$envLines | Set-Content $envTarget -Encoding ASCII

$runScriptTemplate = @'
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$envFile = "__ENVFILE__"
$logDir = "__LOGDIR__"
$workDir = "__INSTALLROOT__"
$exePath = "__EXEPATH__"
$outLog = Join-Path $logDir "edge.out.log"
$errLog = Join-Path $logDir "edge.err.log"
$taskLog = Join-Path $logDir "edge.task.log"

New-Item -ItemType Directory -Force -Path $logDir | Out-Null
Start-Transcript -Path $taskLog -Append | Out-Null
try {
  if (-not (Test-Path $envFile)) {
    throw "edge.env not found: $envFile"
  }
  if (-not (Test-Path $exePath)) {
    throw "edge-agent.exe not found: $exePath"
  }
  New-Item -ItemType File -Force -Path $outLog | Out-Null
  New-Item -ItemType File -Force -Path $errLog | Out-Null

  Get-Content $envFile | ForEach-Object {
    if ($_ -match '^\s*#' -or $_ -notmatch '=') { return }
    $k, $v = $_ -split '=', 2
    [Environment]::SetEnvironmentVariable($k.Trim(), $v.Trim(), 'Process')
  }

  Write-Host ("[runner] starting edge-agent pid-parent={0} exe={1}" -f $PID, $exePath)
  while ($true) {
    $startedAt = Get-Date
    & $exePath 1>> $outLog 2>> $errLog
    $exitCode = $LASTEXITCODE
    $elapsed = (Get-Date) - $startedAt
    Write-Host ("[runner] edge-agent exited code={0} after {1:n1}s; restart in 5s" -f $exitCode, $elapsed.TotalSeconds)
    Start-Sleep -Seconds 5
  }
}
catch {
  $msg = $_ | Out-String
  Add-Content -Path $errLog -Value ("[runner] fatal error`r`n{0}" -f $msg)
  Write-Host ("[runner] fatal error: {0}" -f $_.Exception.Message)
  exit 1
}
finally {
  Stop-Transcript | Out-Null
}
'@
$runScriptContent = $runScriptTemplate
$runScriptContent = $runScriptContent.Replace("__ENVFILE__", $envTarget)
$runScriptContent = $runScriptContent.Replace("__LOGDIR__", $logDir)
$runScriptContent = $runScriptContent.Replace("__INSTALLROOT__", $InstallRoot)
$runScriptContent = $runScriptContent.Replace("__EXEPATH__", $exeTarget)
$runScriptContent | Set-Content $runScript -Encoding ASCII

Remove-ServiceIfExists -Name $ServiceName
Remove-TaskIfExists -Name $TaskName

$outLog = "$logDir\\edge.out.log"
$errLog = "$logDir\\edge.err.log"
$taskLog = "$logDir\\edge.task.log"
switch ($InstallMode) {
    "nssm" {
        Install-NssmService -Name $ServiceName -ScriptPath $runScript -WorkingDir $DataRoot -OutLog $outLog -ErrLog $errLog
        Write-Host "Installed mode: nssm service"
    }
    "task" {
        Install-TaskRunner -Name $TaskName -ScriptPath $runScript -UserName $RunAsUser -PasswordValue $RunAsPassword
        Write-Host "Installed mode: scheduled task"
    }
    "auto" {
        if ($null -ne (Get-Command nssm -ErrorAction SilentlyContinue)) {
            Install-NssmService -Name $ServiceName -ScriptPath $runScript -WorkingDir $DataRoot -OutLog $outLog -ErrLog $errLog
            Write-Host "Installed mode: nssm service"
        } else {
            Install-TaskRunner -Name $TaskName -ScriptPath $runScript -UserName $RunAsUser -PasswordValue $RunAsPassword
            Write-Host "Installed mode: scheduled task (nssm not found)"
        }
    }
}

if ($InstallMode -eq "task" -or ($InstallMode -eq "auto" -and $null -eq (Get-Command nssm -ErrorAction SilentlyContinue))) {
    Start-Sleep -Seconds 2
    $taskInfo = Get-ScheduledTaskInfo -TaskName $TaskName -ErrorAction SilentlyContinue
    if ($null -ne $taskInfo) {
        Write-Host "Task last result: $($taskInfo.LastTaskResult)"
    }
    Start-Sleep -Seconds 2
    $edgeProc = Get-Process -Name "edge-agent" -ErrorAction SilentlyContinue
    if ($null -ne $edgeProc) {
        Write-Host "edge-agent process running: PID=$($edgeProc.Id)"
    } else {
        Write-Warning "edge-agent process not detected yet. Check task log: $taskLog"
    }
}

Write-Host "Installed service/task name: $ServiceName / $TaskName"
Write-Host "Task run as: $RunAsUser"
Write-Host "Env file: $envTarget"
Write-Host "Logs:"
Write-Host "  $logDir\\edge.out.log"
Write-Host "  $logDir\\edge.err.log"
