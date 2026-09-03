param(
    [string]$Version,
    [int]$ConfigSchemaVersion = 1,
    [string]$MinimumCentralVersion = "1.0.0",
    [string]$BinaryPath,
    [string]$SupervisorPath,
    [string]$OutputRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$packageRoot = Split-Path -Path $PSScriptRoot -Parent
$repoRoot = Split-Path -Path (Split-Path -Path $packageRoot -Parent) -Parent
if ([string]::IsNullOrWhiteSpace($OutputRoot)) { $OutputRoot = $packageRoot }
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = (Get-Content -LiteralPath (Join-Path $repoRoot "VERSION") -Raw).Trim()
}
$semanticVersionPattern = '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$'
if ($Version -notmatch $semanticVersionPattern) { throw "Version must be semantic, for example 1.1.0." }
if ($MinimumCentralVersion -notmatch $semanticVersionPattern) { throw "MinimumCentralVersion must be semantic, for example 1.0.0." }
if ($ConfigSchemaVersion -lt 1) { throw "ConfigSchemaVersion must be at least 1." }

if ([string]::IsNullOrWhiteSpace($BinaryPath)) {
    & cargo build --release -p edge-agent --manifest-path (Join-Path $repoRoot "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    $BinaryPath = Join-Path $repoRoot "target\release\edge-agent.exe"
}
# El supervisor viaja en el mismo paquete: es lo que la tarea programada lanza, y el agente
# pasa a ser su hijo. Sin el, install-edge.ps1 aborta con "Missing binary".
if ([string]::IsNullOrWhiteSpace($SupervisorPath)) {
    & cargo build --release -p edge-supervisor --manifest-path (Join-Path $repoRoot "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    $SupervisorPath = Join-Path $repoRoot "target\release\edge-supervisor.exe"
}

$binarySource = [IO.Path]::GetFullPath($BinaryPath)
if (-not (Test-Path -LiteralPath $binarySource -PathType Leaf)) { throw "Binary not found: $binarySource" }
$supervisorSource = [IO.Path]::GetFullPath($SupervisorPath)
if (-not (Test-Path -LiteralPath $supervisorSource -PathType Leaf)) { throw "Supervisor binary not found: $supervisorSource" }
$outputFull = [IO.Path]::GetFullPath($OutputRoot)
$binDir = Join-Path $outputFull "bin"
$scriptsDir = Join-Path $outputFull "scripts"
$binaryTarget = Join-Path $binDir "edge-agent.exe"
$supervisorTarget = Join-Path $binDir "edge-supervisor.exe"
$updaterSource = Join-Path $packageRoot "scripts\update-edge.ps1"
$updaterTarget = Join-Path $scriptsDir "update-edge.ps1"
$manifestTarget = Join-Path $outputFull "release-manifest.json"
$manifestTemp = Join-Path $outputFull (".release-manifest.{0}.tmp" -f $PID)

New-Item -ItemType Directory -Force -Path $binDir, $scriptsDir | Out-Null
if (-not $binarySource.Equals([IO.Path]::GetFullPath($binaryTarget), [StringComparison]::OrdinalIgnoreCase)) {
    Copy-Item -LiteralPath $binarySource -Destination $binaryTarget -Force
}
if (-not $supervisorSource.Equals([IO.Path]::GetFullPath($supervisorTarget), [StringComparison]::OrdinalIgnoreCase)) {
    Copy-Item -LiteralPath $supervisorSource -Destination $supervisorTarget -Force
}
if (-not ([IO.Path]::GetFullPath($updaterSource)).Equals([IO.Path]::GetFullPath($updaterTarget), [StringComparison]::OrdinalIgnoreCase)) {
    Copy-Item -LiteralPath $updaterSource -Destination $updaterTarget -Force
}
$sha256 = (Get-FileHash -LiteralPath $binaryTarget -Algorithm SHA256).Hash.ToLowerInvariant()
$supervisorSha256 = (Get-FileHash -LiteralPath $supervisorTarget -Algorithm SHA256).Hash.ToLowerInvariant()

$manifest = [ordered]@{
    manifest_version = 1
    version = $Version
    config_schema_version = $ConfigSchemaVersion
    minimum_central_version = $MinimumCentralVersion
    binary = [ordered]@{
        path = "bin/edge-agent.exe"
        sha256 = $sha256
    }
    # Clave aparte y no dentro de `binary`: update-edge.ps1 exige que binary.path sea
    # exactamente bin/edge-agent.exe, y valida campo a campo, asi que ignora esta sin
    # quejarse. Hoy el updater no reemplaza el supervisor -- cambiarlo pide reinstalar --
    # pero el hash queda declarado para poder verificar que se desplego.
    supervisor = [ordered]@{
        path = "bin/edge-supervisor.exe"
        sha256 = $supervisorSha256
    }
}

try {
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestTemp -Encoding UTF8
    Move-Item -LiteralPath $manifestTemp -Destination $manifestTarget -Force
} finally {
    if (Test-Path -LiteralPath $manifestTemp) { Remove-Item -LiteralPath $manifestTemp -Force }
}

Write-Host "Edge release package generated: $outputFull"
Write-Host "Version: $Version"
Write-Host "SHA-256 edge-agent:      $sha256"
Write-Host "SHA-256 edge-supervisor: $supervisorSha256"
