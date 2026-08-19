
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
