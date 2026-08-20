# Internal wrapper functions for external commands (mockable for testing)
function Invoke-Ssh {
    param([Parameter(ValueFromRemainingArguments=$true)]$Args)
    & ssh @Args
}

function Invoke-Scp {
    param([Parameter(ValueFromRemainingArguments=$true)]$Args)
    & scp @Args
}

function Get-CurrentImageTag {
    param(
        [Parameter(Mandatory)][string]$EnvContent,
        [Parameter(Mandatory)][string]$VarName
    )
    $pattern = "(?m)^$([regex]::Escape($VarName))=(.*)$"
    $match = [regex]::Match($EnvContent, $pattern)
    if (-not $match.Success) {
        throw "Variable '$VarName' not found in env content"
    }
    return $match.Groups[1].Value.Trim()
}

function Set-ImageTag {
    param(
        [Parameter(Mandatory)][string]$EnvContent,
        [Parameter(Mandatory)][string]$VarName,
        [Parameter(Mandatory)][string]$NewValue
    )
    $lines = $EnvContent -split "`r?`n"
    $found = $false
    $result = for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match "^$([regex]::Escape($VarName))=") {
            $found = $true
            "$VarName=$NewValue"
        } else {
            $lines[$i]
        }
    }
    if (-not $found) {
        throw "Variable '$VarName' not found in env content; refusing to append a new one"
    }
    return ($result -join "`r`n")
}

function Test-ServiceHealthy {
    param(
        [Parameter(Mandatory)][string]$Url,
        [int]$MaxAttempts = 30,
        [int]$PollIntervalSeconds = 2
    )
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        try {
            $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 5
            if ($response.StatusCode -eq 200) {
                return $true
            }
        } catch {
            # not up yet, keep polling
        }
        if ($attempt -lt $MaxAttempts) {
            Start-Sleep -Seconds $PollIntervalSeconds
        }
    }
    return $false
}

function Invoke-RemoteCommand {
    param(
        [Parameter(Mandatory)][string]$TargetHost,
        [Parameter(Mandatory)][string]$SshUser,
        [Parameter(Mandatory)][string]$SshKeyPath,
        [Parameter(Mandatory)][string]$Command
    )
    $sshArgs = @("-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-i", $SshKeyPath, "$SshUser@$TargetHost", $Command)
    $output = Invoke-Ssh @sshArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Remote command failed (exit $LASTEXITCODE): $Command`n$output"
    }
    return $output
}

function Copy-ToRemote {
    param(
        [Parameter(Mandatory)][string]$TargetHost,
        [Parameter(Mandatory)][string]$SshUser,
        [Parameter(Mandatory)][string]$SshKeyPath,
        [Parameter(Mandatory)][string]$LocalPath,
        [Parameter(Mandatory)][string]$RemotePath
    )
    $scpArgs = @("-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-i", $SshKeyPath, $LocalPath, "${SshUser}@${TargetHost}:${RemotePath}")
    Invoke-Scp @scpArgs
    if ($LASTEXITCODE -ne 0) {
        throw "File copy failed: $LocalPath -> ${TargetHost}:${RemotePath}"
    }
}

# Orchestrates a full deploy of Update Protocol v1: copy the image tar to the remote
# host, load it, swap the .env-referenced tag, restart via docker compose, and poll
# health. On a failed health check it automatically rolls back to the previous image
# tag. NOTE: this function throws in BOTH failure paths -- even when the rollback
# itself succeeds -- because a successful automatic rollback still means the *new*
# version never shipped, and the caller (Task 12/13's CI workflow step) must fail the
# job loudly in both cases so a human notices. Only a healthy deploy of NewImageRef
# returns normally.
function Invoke-DockerServiceDeploy {
    param(
        [Parameter(Mandatory)][ValidateSet("central-server", "web-ui")][string]$Service,
        [Parameter(Mandatory)][string]$TargetHost,
        [Parameter(Mandatory)][string]$SshUser,
        [Parameter(Mandatory)][string]$SshKeyPath,
        [Parameter(Mandatory)][string]$ImageTarLocalPath,
        [Parameter(Mandatory)][string]$NewImageRef,
        [Parameter(Mandatory)][string]$HealthUrl,
        [string]$RemoteComposeDir = "C:\ifascada-central",
        [int]$HealthMaxAttempts = 30,
        [int]$HealthPollIntervalSeconds = 2
    )

    $envVarName = if ($Service -eq "central-server") { "CENTRAL_IMAGE" } else { "WEB_UI_IMAGE" }
    # Backslashes matter here, not just style: the remote command shell for a non-interactive
    # `ssh host "..."` invocation on this Windows host is cmd.exe, and cmd's internal `type`
    # command fails to resolve a forward-slash path ("system cannot find the file specified")
    # even though docker.exe and PowerShell -Command both accept either separator fine.
    $remoteTarPath = "$RemoteComposeDir\deploy-$Service.tar"
    $remoteEnvPath = "$RemoteComposeDir\.env"

    Write-Host "[$Service] Copying image tar to $TargetHost..."
    Copy-ToRemote -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath -LocalPath $ImageTarLocalPath -RemotePath $remoteTarPath

    Write-Host "[$Service] Loading image on remote host..."
    Invoke-RemoteCommand -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath -Command "docker load -i $remoteTarPath" | Out-Null

    $currentEnvContent = (Invoke-RemoteCommand -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath -Command "type `"$remoteEnvPath`"") -join "`r`n"
    # The real .env on this host was written with a leading UTF-8 BOM (U+FEFF). Left in place,
    # it lands on the same line as the first variable, so the multiline ^VarName= regex in
    # Get-CurrentImageTag/Set-ImageTag never matches the first line of the file. Strip it
    # defensively -- we don't control how this file gets (re)written outside this script.
    $currentEnvContent = $currentEnvContent.TrimStart([char]0xFEFF)
    $previousImageRef = Get-CurrentImageTag -EnvContent $currentEnvContent -VarName $envVarName
    Write-Host "[$Service] Current image: $previousImageRef -> deploying: $NewImageRef"

    function Set-RemoteImageRefAndRestart([string]$ImageRef) {
        $newEnvContent = Set-ImageTag -EnvContent $currentEnvContent -VarName $envVarName -NewValue $ImageRef
        $escaped = $newEnvContent -replace '"', '""'
        Invoke-RemoteCommand -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath `
            -Command "powershell -NoProfile -Command `"Set-Content -Path '$remoteEnvPath' -Value \`"$escaped\`" -NoNewline`"" | Out-Null
        Invoke-RemoteCommand -TargetHost $TargetHost -SshUser $SshUser -SshKeyPath $SshKeyPath `
            -Command "cd $RemoteComposeDir && docker compose up -d $Service" | Out-Null
    }

    Set-RemoteImageRefAndRestart -ImageRef $NewImageRef

    Write-Host "[$Service] Waiting for health check at $HealthUrl..."
    $healthy = Test-ServiceHealthy -Url $HealthUrl -MaxAttempts $HealthMaxAttempts -PollIntervalSeconds $HealthPollIntervalSeconds

    if ($healthy) {
        Write-Host "[$Service] Deploy succeeded: $NewImageRef is healthy."
        return
    }

    Write-Host "[$Service] Health check FAILED. Rolling back to $previousImageRef..."
    Set-RemoteImageRefAndRestart -ImageRef $previousImageRef
    $rolledBack = Test-ServiceHealthy -Url $HealthUrl -MaxAttempts $HealthMaxAttempts -PollIntervalSeconds $HealthPollIntervalSeconds
    if (-not $rolledBack) {
        throw "[$Service] Rollback to $previousImageRef ALSO failed health check. Manual intervention required."
    }
    throw "[$Service] Deploy of $NewImageRef failed health check; automatically rolled back to $previousImageRef."
}
