
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
