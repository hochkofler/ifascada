# scripts/deploy-docker-service.ps1
param(
    [Parameter(Mandatory)][ValidateSet("central-server", "web-ui")][string]$Service,
    [Parameter(Mandatory)][string]$TargetHost,
    [Parameter(Mandatory)][string]$SshUser,
    [Parameter(Mandatory)][string]$SshKeyPath,
    [Parameter(Mandatory)][string]$ImageTarLocalPath,
    [Parameter(Mandatory)][string]$NewImageRef,
    [Parameter(Mandatory)][string]$HealthUrl,
    [string]$RemoteComposeDir = "C:/ifascada-central",
    [int]$HealthMaxAttempts = 30,
    [int]$HealthPollIntervalSeconds = 2
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "lib\DeployDockerService.ps1")

Invoke-DockerServiceDeploy -Service $Service -TargetHost $TargetHost -SshUser $SshUser `
    -SshKeyPath $SshKeyPath -ImageTarLocalPath $ImageTarLocalPath -NewImageRef $NewImageRef `
    -HealthUrl $HealthUrl -RemoteComposeDir $RemoteComposeDir `
    -HealthMaxAttempts $HealthMaxAttempts -HealthPollIntervalSeconds $HealthPollIntervalSeconds
