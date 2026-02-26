param(
    [string[]]$Ports = @("8088", "3015"),
    [int]$TimeoutSeconds = 20,
    [int]$PollMs = 500
)

$ErrorActionPreference = "Stop"

function Get-ListeningOwners([int]$Port) {
    return Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess -Unique
}

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while ((Get-Date) -lt $deadline) {
    $busy = @()
    foreach ($p in $Ports) {
        $pn = [int]$p
        $owners = Get-ListeningOwners -Port $pn
        if ($null -ne $owners -and $owners.Count -gt 0) {
            $busy += [PSCustomObject]@{
                Port = $pn
                Owners = ($owners -join ",")
            }
        }
    }
    if ($busy.Count -eq 0) {
        Write-Host "PORTS_FREE"
        exit 0
    }
    Start-Sleep -Milliseconds $PollMs
}

Write-Host "PORTS_STILL_BUSY"
foreach ($p in $Ports) {
    $pn = [int]$p
    $owners = Get-ListeningOwners -Port $pn
    if ($null -ne $owners -and $owners.Count -gt 0) {
        Write-Host (" - port {0}: pid(s) {1}" -f $pn, ($owners -join ","))
    }
}
exit 1
