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
