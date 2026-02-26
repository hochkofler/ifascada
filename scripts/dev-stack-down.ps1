param(
    [string]$ComposeFile = "docker-compose.scada.yml",
    [string]$SimComposeFile = "docker-compose.edge-sim.yml",
    [switch]$WithSimEdges,
    [switch]$RemoveVolumes
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $ComposeFile)) {
    throw "Compose file not found: $ComposeFile"
}
if ($WithSimEdges -and -not (Test-Path $SimComposeFile)) {
    throw "Compose file not found: $SimComposeFile"
}

Write-Host "Stopping SCADA infrastructure stack from $ComposeFile ..."
if ($RemoveVolumes) {
    if ($WithSimEdges) {
        docker compose -f $ComposeFile -f $SimComposeFile down -v
    } else {
        docker compose -f $ComposeFile down -v
    }
} else {
    if ($WithSimEdges) {
        docker compose -f $ComposeFile -f $SimComposeFile down
    } else {
        docker compose -f $ComposeFile down
    }
}

Write-Host "Done."
