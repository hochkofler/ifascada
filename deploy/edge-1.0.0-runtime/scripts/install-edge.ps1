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

    # El orden importa: primero el padre, despues el hijo. Matar el edge-agent antes que su
    # supervisor solo consigue que el supervisor lo relance de inmediato, sobre el binario
    # que estamos por reemplazar.
    Get-Process -Name "edge-supervisor" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

    # Instalaciones viejas, anteriores al supervisor: el lanzador era un powershell.
    $legacyRunners = Get-CimInstance Win32_Process -Filter "Name = 'powershell.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -like "*run-edge.ps1*" }
    foreach ($proc in $legacyRunners) {
        Stop-Process -Id $proc.ProcessId -Force -ErrorAction SilentlyContinue
    }

    Start-Sleep -Seconds 1

    # Red de seguridad. Con el Job Object el agente ya deberia haber caido junto al
    # supervisor; esto cubre el caso de un agente huerfano de una instalacion anterior.
    Get-Process -Name "edge-agent" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
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

function Install-TaskRunner([string]$Name, [string]$SupervisorPath, [string]$EnvFile, [string]$UserName, [SecureString]$PasswordValue) {
    Remove-TaskIfExists -Name $Name
    # The supervisor is what the task launches now; the agent is its child. A scheduled
    # task cannot hand environment variables to what it launches, so edge.env travels as
    # an argument.
    $taskAction = New-ScheduledTaskAction -Execute $SupervisorPath -Argument "--env-file `"$EnvFile`""
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

function Install-NssmService([string]$Name, [string]$SupervisorPath, [string]$EnvFile, [string]$WorkingDir, [string]$OutLog, [string]$ErrLog) {
    $nssm = Get-Command nssm -ErrorAction SilentlyContinue
    if ($null -eq $nssm) {
        throw "InstallMode=nssm but nssm is not installed."
    }
    Remove-ServiceIfExists -Name $Name
    & $nssm.Source install $Name $SupervisorPath "--env-file `"$EnvFile`""
    & $nssm.Source set $Name AppDirectory $WorkingDir
    & $nssm.Source set $Name AppStdout $OutLog
    & $nssm.Source set $Name AppStderr $ErrLog
    & $nssm.Source set $Name Start SERVICE_AUTO_START
    & $nssm.Source start $Name
}

Assert-Admin

$pkgRoot = Split-Path -Path $PSScriptRoot -Parent
$binSource = Join-Path $pkgRoot "bin\\edge-agent.exe"
$supervisorSource = Join-Path $pkgRoot "bin\\edge-supervisor.exe"
$bootstrapSource = Join-Path $pkgRoot "config\\bootstrap.example.json"
$envTemplate = Join-Path $pkgRoot "config\\edge.env.example"

if (-not (Test-Path $binSource)) {
    throw "Missing binary: $binSource"
}
if (-not (Test-Path $supervisorSource)) {
    throw "Missing binary: $supervisorSource"
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
# Beside the agent, which is where the supervisor looks for it by default.
$supervisorTarget = Join-Path $InstallRoot "edge-supervisor.exe"

New-Item -ItemType Directory -Force -Path $InstallRoot | Out-Null
New-Item -ItemType Directory -Force -Path $DataRoot | Out-Null
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
New-Item -ItemType Directory -Force -Path $configDir | Out-Null

Stop-EdgeRuntime -TaskToStop $TaskName
Copy-Item $binSource $exeTarget -Force
Copy-Item $supervisorSource $supervisorTarget -Force
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
    "EDGE_CONFIG_APPLY_RECEIPT_PATH=$DataRoot\\config_apply_receipt.json",
    "",
    "# Donde el supervisor escribe la salida del agente. El script de diagnostico",
    "# edge-diagnose-and-restart.ps1 lee edge.err.log de aqui.",
    "EDGE_SUPERVISOR_LOG_DIR=$logDir"
)
$envLines | Set-Content $envTarget -Encoding ASCII

# run-edge.ps1 ya no se genera: el supervisor lo reemplaza. Una copia vieja ademas es
# peligrosa -- correrla a mano levantaria un segundo agente junto al supervisado, ambos
# peleando por los mismos puertos COM y el mismo MQTT client_id.
if (Test-Path $runScript) {
    Remove-Item $runScript -Force
    Write-Host "Removed the obsolete run-edge.ps1"
}

Remove-ServiceIfExists -Name $ServiceName
Remove-TaskIfExists -Name $TaskName

$outLog = "$logDir\\edge.out.log"
$errLog = "$logDir\\edge.err.log"
$taskLog = "$logDir\\edge.task.log"
switch ($InstallMode) {
    "nssm" {
        Install-NssmService -Name $ServiceName -SupervisorPath $supervisorTarget -EnvFile $envTarget -WorkingDir $DataRoot -OutLog $outLog -ErrLog $errLog
        Write-Host "Installed mode: nssm service"
    }
    "task" {
        Install-TaskRunner -Name $TaskName -SupervisorPath $supervisorTarget -EnvFile $envTarget -UserName $RunAsUser -PasswordValue $RunAsPassword
        Write-Host "Installed mode: scheduled task"
    }
    "auto" {
        if ($null -ne (Get-Command nssm -ErrorAction SilentlyContinue)) {
            Install-NssmService -Name $ServiceName -SupervisorPath $supervisorTarget -EnvFile $envTarget -WorkingDir $DataRoot -OutLog $outLog -ErrLog $errLog
            Write-Host "Installed mode: nssm service"
        } else {
            Install-TaskRunner -Name $TaskName -SupervisorPath $supervisorTarget -EnvFile $envTarget -UserName $RunAsUser -PasswordValue $RunAsPassword
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
