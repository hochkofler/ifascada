param(
    [string]$PackageRoot = (Split-Path -Path $PSScriptRoot -Parent),
    [string]$ServiceName = "ifascada-edge",
    [string]$TaskName = "ifascada-edge",
    [string]$TaskPath = "\",
    [ValidateSet("auto", "service", "task", "test")]
    [string]$RuntimeMode = "auto",
    [string]$InstallRoot = "C:\Program Files\ifascada\edge",
    [string]$DataRoot = "C:\ProgramData\ifascada\edge",
    [int]$SupportedConfigSchemaVersion = 1,
    [ValidateRange(0, 300)]
    [int]$HealthTimeoutSeconds = 20,
    [ScriptBlock]$HealthCheckScript,
    [string]$TestRuntimeEventLog,
    [switch]$TestSkipAdminCheck
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Run this script as Administrator."
    }
}

function Get-FullPath([string]$Path) {
    return [IO.Path]::GetFullPath($Path).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
}

function Assert-SafeRuntimeName([string]$Name, [string]$Description) {
    if ([string]::IsNullOrWhiteSpace($Name) -or $Name -notmatch '^[0-9A-Za-z_. -]+$') {
        throw "$Description contains unsupported or wildcard characters."
    }
}

function Assert-SafeTaskPath([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path) -or -not $Path.StartsWith("\") -or -not $Path.EndsWith("\") -or $Path.IndexOfAny([char[]]'*?[]') -ge 0) {
        throw "TaskPath must be an exact rooted task folder such as \ and cannot contain wildcards."
    }
}

function Test-JsonInteger($Value) {
    return ($Value -is [int16] -or $Value -is [int32] -or $Value -is [int64])
}

function Assert-SemanticVersion([string]$Version, [string]$Description) {
    if ([string]::IsNullOrWhiteSpace($Version) -or $Version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$') {
        throw "$Description must be a semantic version such as 1.1.0."
    }
}

function Test-PathsOverlap([string]$First, [string]$Second) {
    $firstFull = (Get-FullPath $First) + [IO.Path]::DirectorySeparatorChar
    $secondFull = (Get-FullPath $Second) + [IO.Path]::DirectorySeparatorChar
    return $firstFull.StartsWith($secondFull, [StringComparison]::OrdinalIgnoreCase) -or
        $secondFull.StartsWith($firstFull, [StringComparison]::OrdinalIgnoreCase)
}

function Assert-NotFileSystemRoot([string]$Path, [string]$Description) {
    $full = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($full)
    if ($full.TrimEnd('\', '/') -eq $root.TrimEnd('\', '/')) {
        throw "$Description cannot be a filesystem root."
    }
}

function Assert-NoReparsePoints([string]$Path, [string]$Description) {
    $current = Get-FullPath $Path
    while (-not [string]::IsNullOrWhiteSpace($current)) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "$Description contains a reparse point: $current"
            }
        }
        $parent = Split-Path -Path $current -Parent
        if ([string]::IsNullOrWhiteSpace($parent) -or $parent -eq $current) { break }
        $current = $parent
    }
}

function Assert-ChildPath([string]$Parent, [string]$Child, [string]$Description) {
    $parentPrefix = (Get-FullPath $Parent) + [IO.Path]::DirectorySeparatorChar
    $childFull = Get-FullPath $Child
    if (-not $childFull.StartsWith($parentPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Description must stay inside $Parent"
    }
    return $childFull
}

function Get-ExactEdgeProcesses([string]$ExecutablePath) {
    $expected = Get-FullPath $ExecutablePath
    return @(Get-CimInstance Win32_Process -Filter "Name = 'edge-agent.exe'" -ErrorAction SilentlyContinue |
        Where-Object {
            $_.ExecutablePath -and (Get-FullPath $_.ExecutablePath).Equals($expected, [StringComparison]::OrdinalIgnoreCase)
        })
}

function Get-ExactService([string]$Name) {
    $matches = @(Get-Service -ErrorAction Stop | Where-Object { $_.Name.Equals($Name, [StringComparison]::OrdinalIgnoreCase) })
    if ($matches.Count -gt 1) { throw "More than one service matched exact name '$Name'." }
    if ($matches.Count -eq 0) { return $null }
    return $matches[0]
}

function Get-ExactTask([string]$Name, [string]$Path) {
    $matches = @(Get-ScheduledTask -ErrorAction Stop | Where-Object {
        $_.TaskName.Equals($Name, [StringComparison]::OrdinalIgnoreCase) -and
        $_.TaskPath.Equals($Path, [StringComparison]::OrdinalIgnoreCase)
    })
    if ($matches.Count -gt 1) { throw "More than one scheduled task matched exact path '$Path$Name'." }
    if ($matches.Count -eq 0) { return $null }
    return $matches[0]
}

function Resolve-EdgeRuntime([string]$RequestedMode, [string]$Service, [string]$Task, [string]$ScheduledTaskPath) {
    if ($RequestedMode -eq "test") { return [pscustomobject]@{ Mode = "test"; Handle = $null } }
    if ($RequestedMode -eq "service" -or $RequestedMode -eq "auto") {
        $serviceObject = Get-ExactService -Name $Service
        if ($null -ne $serviceObject) { return [pscustomobject]@{ Mode = "service"; Handle = $serviceObject } }
        if ($RequestedMode -eq "service") { throw "Service '$Service' does not exist." }
    }
    $taskObject = Get-ExactTask -Name $Task -Path $ScheduledTaskPath
    if ($null -ne $taskObject) { return [pscustomobject]@{ Mode = "task"; Handle = $taskObject } }
    throw "Neither service '$Service' nor scheduled task '$ScheduledTaskPath$Task' exists."
}

function Stop-EdgeRuntime($Runtime, [string]$ExecutablePath, [string]$TestEventLog, [string]$Task, [string]$ScheduledTaskPath) {
    switch ($Runtime.Mode) {
        "service" {
            $svc = $Runtime.Handle
            if ($svc.Status -ne "Stopped") {
                Stop-Service -InputObject $svc -Force -ErrorAction Stop
                $svc.WaitForStatus("Stopped", [TimeSpan]::FromSeconds(15))
            }
        }
        "task" {
            Stop-ScheduledTask -InputObject $Runtime.Handle -ErrorAction Stop
            $deadline = [DateTime]::UtcNow.AddSeconds(15)
            do {
                $currentTask = Get-ExactTask -Name $Task -Path $ScheduledTaskPath
                if ($null -eq $currentTask) { throw "Scheduled task '$ScheduledTaskPath$Task' disappeared while stopping." }
                if ($currentTask.State -ne "Running") { break }
                if ([DateTime]::UtcNow -ge $deadline) { throw "Scheduled task '$ScheduledTaskPath$Task' did not stop within 15 seconds." }
                Start-Sleep -Milliseconds 250
            } while ($true)
        }
        "test" {
            if (-not [string]::IsNullOrWhiteSpace($TestEventLog)) { Add-Content -LiteralPath $TestEventLog -Value "stop" }
            return
        }
    }

    foreach ($proc in (Get-ExactEdgeProcesses -ExecutablePath $ExecutablePath)) {
        Stop-Process -Id $proc.ProcessId -Force -ErrorAction Stop
    }
}

function Start-EdgeRuntime($Runtime, [string]$TestEventLog) {
    switch ($Runtime.Mode) {
        "service" { Start-Service -InputObject $Runtime.Handle -ErrorAction Stop }
        "task" { Start-ScheduledTask -InputObject $Runtime.Handle -ErrorAction Stop }
        "test" {
            if (-not [string]::IsNullOrWhiteSpace($TestEventLog)) { Add-Content -LiteralPath $TestEventLog -Value "start" }
            return
        }
    }
}

function Test-EdgeHealthy([string]$Mode, [string]$ExecutablePath, [int]$TimeoutSeconds, [ScriptBlock]$CustomCheck) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ($null -ne $CustomCheck) {
            if (& $CustomCheck) { return $true }
        } elseif ($Mode -eq "test") {
            return $true
        } elseif ((Get-ExactEdgeProcesses -ExecutablePath $ExecutablePath).Count -gt 0) {
            return $true
        }
        if ([DateTime]::UtcNow -ge $deadline) { break }
        Start-Sleep -Milliseconds 500
    } while ($true)
    return $false
}

function Get-BackupDirectory([string]$ReleasesRoot, [string]$Version) {
    if ($Version -ne "unknown") { Assert-SemanticVersion -Version $Version -Description "Installed version" }
    $versionRoot = Assert-ChildPath -Parent $ReleasesRoot -Child (Join-Path $ReleasesRoot $Version) -Description "Version backup directory"
    $snapshotName = "{0}-{1}" -f ([DateTime]::UtcNow.ToString("yyyyMMddTHHmmssfffZ")), ([Guid]::NewGuid().ToString("N"))
    return Assert-ChildPath -Parent $versionRoot -Child (Join-Path $versionRoot $snapshotName) -Description "Backup snapshot directory"
}

$packageFull = Get-FullPath $PackageRoot
$installFull = Get-FullPath $InstallRoot
$dataFull = Get-FullPath $DataRoot
Assert-NotFileSystemRoot -Path $packageFull -Description "PackageRoot"
Assert-NotFileSystemRoot -Path $installFull -Description "InstallRoot"
Assert-NotFileSystemRoot -Path $dataFull -Description "DataRoot"
Assert-SafeRuntimeName -Name $ServiceName -Description "ServiceName"
Assert-SafeRuntimeName -Name $TaskName -Description "TaskName"
Assert-SafeTaskPath -Path $TaskPath
$testHarnessEnabled = $env:IFASCADA_UPDATER_TEST_MODE -eq "1"
if ($RuntimeMode -eq "test" -and -not $testHarnessEnabled) {
    throw "RuntimeMode=test is disabled outside the updater test harness."
}
if (($null -ne $HealthCheckScript -or -not [string]::IsNullOrWhiteSpace($TestRuntimeEventLog) -or $TestSkipAdminCheck) -and -not $testHarnessEnabled) {
    throw "Test hooks are disabled outside the updater test harness."
}
if (Test-PathsOverlap -First $packageFull -Second $installFull) { throw "PackageRoot and InstallRoot cannot overlap." }
if (Test-PathsOverlap -First $packageFull -Second $dataFull) { throw "PackageRoot and DataRoot cannot overlap." }
if (Test-PathsOverlap -First $installFull -Second $dataFull) { throw "InstallRoot and DataRoot cannot overlap." }
Assert-NoReparsePoints -Path $packageFull -Description "PackageRoot"
Assert-NoReparsePoints -Path $installFull -Description "InstallRoot"
Assert-NoReparsePoints -Path $dataFull -Description "DataRoot"

$manifestSource = Assert-ChildPath -Parent $packageFull -Child (Join-Path $packageFull "release-manifest.json") -Description "Release manifest"
if (-not (Test-Path -LiteralPath $manifestSource -PathType Leaf)) {
    throw "Missing release manifest: $manifestSource"
}
Assert-NoReparsePoints -Path $manifestSource -Description "Release manifest"

$manifestJson = Get-Content -LiteralPath $manifestSource -Raw
$manifest = $manifestJson | ConvertFrom-Json
if (-not (Test-JsonInteger $manifest.manifest_version) -or $manifest.manifest_version -ne 1) {
    throw "manifest_version must be the integer 1."
}
if ($manifest.version -isnot [string]) { throw "Manifest version must be a string." }
Assert-SemanticVersion -Version $manifest.version -Description "Manifest version"
if (-not (Test-JsonInteger $manifest.config_schema_version) -or $manifest.config_schema_version -lt 1) {
    throw "config_schema_version must be a positive integer."
}
if ($manifest.config_schema_version -gt $SupportedConfigSchemaVersion) {
    throw "Config schema $($manifest.config_schema_version) is newer than supported schema $SupportedConfigSchemaVersion."
}
if ($manifest.minimum_central_version -isnot [string]) { throw "minimum_central_version must be a string." }
Assert-SemanticVersion -Version $manifest.minimum_central_version -Description "minimum_central_version"
if ($manifest.binary.path -isnot [string] -or ($manifest.binary.path -replace '/', '\') -cne 'bin\edge-agent.exe') {
    throw "Manifest binary.path must be exactly bin/edge-agent.exe."
}
if ($manifest.binary.sha256 -isnot [string] -or $manifest.binary.sha256 -notmatch '^[0-9A-Fa-f]{64}$') {
    throw "Manifest binary.sha256 is invalid."
}

$binarySource = Assert-ChildPath -Parent $packageFull -Child (Join-Path $packageFull ([string]$manifest.binary.path)) -Description "Release binary"
if (-not (Test-Path -LiteralPath $binarySource -PathType Leaf)) { throw "Missing release binary: $binarySource" }
Assert-NoReparsePoints -Path $binarySource -Description "Release binary"
$actualHash = (Get-FileHash -LiteralPath $binarySource -Algorithm SHA256).Hash.ToLowerInvariant()
$expectedHash = ([string]$manifest.binary.sha256).ToLowerInvariant()
if ($actualHash -ne $expectedHash) { throw "Release binary SHA-256 mismatch." }

$exeTarget = Assert-ChildPath -Parent $installFull -Child (Join-Path $installFull "edge-agent.exe") -Description "Installed binary"
$installedManifest = Assert-ChildPath -Parent $installFull -Child (Join-Path $installFull "release-manifest.json") -Description "Installed manifest"
if (-not (Test-Path -LiteralPath $exeTarget -PathType Leaf)) { throw "Installed binary not found: $exeTarget" }
Assert-NoReparsePoints -Path $exeTarget -Description "Installed binary"
Assert-NoReparsePoints -Path $installedManifest -Description "Installed manifest"
if ($RuntimeMode -ne "test" -and -not $TestSkipAdminCheck) { Assert-Admin }
$runtime = Resolve-EdgeRuntime -RequestedMode $RuntimeMode -Service $ServiceName -Task $TaskName -ScheduledTaskPath $TaskPath

$oldVersion = "unknown"
if (Test-Path -LiteralPath $installedManifest -PathType Leaf) {
    $oldManifest = Get-Content -LiteralPath $installedManifest -Raw | ConvertFrom-Json
    if ($oldManifest.version -isnot [string]) { throw "Installed manifest version must be a string." }
    Assert-SemanticVersion -Version $oldManifest.version -Description "Installed version"
    $oldVersion = $oldManifest.version
}

$releasesRoot = Assert-ChildPath -Parent $installFull -Child (Join-Path $installFull "releases") -Description "Releases directory"
Assert-NoReparsePoints -Path $releasesRoot -Description "Releases directory"
$backupDir = Get-BackupDirectory -ReleasesRoot $releasesRoot -Version $oldVersion
$backupBinary = Assert-ChildPath -Parent $backupDir -Child (Join-Path $backupDir "edge-agent.exe") -Description "Backup binary"
$backupManifest = Assert-ChildPath -Parent $backupDir -Child (Join-Path $backupDir "release-manifest.json") -Description "Backup manifest"
if ((Get-FullPath $backupBinary).Equals((Get-FullPath $exeTarget), [StringComparison]::OrdinalIgnoreCase)) {
    throw "Backup binary must be distinct from the installed binary."
}
$stageId = [Guid]::NewGuid().ToString("N")
$stagedBinary = Assert-ChildPath -Parent $installFull -Child (Join-Path $installFull (".edge-agent.{0}.new" -f $stageId)) -Description "Staged binary"
$stagedManifest = Assert-ChildPath -Parent $installFull -Child (Join-Path $installFull (".release-manifest.{0}.new" -f $stageId)) -Description "Staged manifest"
if (Test-Path -LiteralPath $stagedBinary) { throw "Refusing to reuse existing staged binary: $stagedBinary" }
if (Test-Path -LiteralPath $stagedManifest) { throw "Refusing to reuse existing staged manifest: $stagedManifest" }
Assert-NoReparsePoints -Path $stagedBinary -Description "Staged binary"
Assert-NoReparsePoints -Path $stagedManifest -Description "Staged manifest"
$hadInstalledManifest = Test-Path -LiteralPath $installedManifest -PathType Leaf
$runtimeMutationAttempted = $false
$replacementStarted = $false

try {
    $runtimeMutationAttempted = $true
    Stop-EdgeRuntime -Runtime $runtime -ExecutablePath $exeTarget -TestEventLog $TestRuntimeEventLog -Task $TaskName -ScheduledTaskPath $TaskPath

    New-Item -ItemType Directory -Force -Path (Split-Path -Path $backupDir -Parent) | Out-Null
    New-Item -ItemType Directory -Path $backupDir | Out-Null
    Assert-NoReparsePoints -Path $backupDir -Description "Backup snapshot"
    Assert-NoReparsePoints -Path $backupBinary -Description "Backup binary"
    Assert-NoReparsePoints -Path $backupManifest -Description "Backup manifest"
    Copy-Item -LiteralPath $exeTarget -Destination $backupBinary
    Assert-NoReparsePoints -Path $backupBinary -Description "Backup binary"
    if ($hadInstalledManifest) {
        Copy-Item -LiteralPath $installedManifest -Destination $backupManifest
        Assert-NoReparsePoints -Path $backupManifest -Description "Backup manifest"
    }
    if ((Get-FileHash -LiteralPath $backupBinary -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $exeTarget -Algorithm SHA256).Hash) {
        throw "Backup binary verification failed."
    }
    if ($hadInstalledManifest -and (Get-FileHash -LiteralPath $backupManifest -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $installedManifest -Algorithm SHA256).Hash) {
        throw "Backup manifest verification failed."
    }

    Copy-Item -LiteralPath $binarySource -Destination $stagedBinary -Force
    Assert-NoReparsePoints -Path $stagedBinary -Description "Staged binary"
    $stagedHash = (Get-FileHash -LiteralPath $stagedBinary -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($stagedHash -ne $expectedHash) { throw "Staged binary SHA-256 mismatch." }
    [IO.File]::WriteAllText($stagedManifest, $manifestJson, (New-Object System.Text.UTF8Encoding($false)))
    Assert-NoReparsePoints -Path $stagedManifest -Description "Staged manifest"
    Assert-NoReparsePoints -Path $exeTarget -Description "Installed binary"
    Assert-NoReparsePoints -Path $installedManifest -Description "Installed manifest"
    if ((Get-FileHash -LiteralPath $stagedBinary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedHash) {
        throw "Staged binary changed before replacement."
    }
    $replacementStarted = $true
    Move-Item -LiteralPath $stagedBinary -Destination $exeTarget -Force
    Move-Item -LiteralPath $stagedManifest -Destination $installedManifest -Force
    Start-EdgeRuntime -Runtime $runtime -TestEventLog $TestRuntimeEventLog

    if (-not (Test-EdgeHealthy -Mode $runtime.Mode -ExecutablePath $exeTarget -TimeoutSeconds $HealthTimeoutSeconds -CustomCheck $HealthCheckScript)) {
        throw "Updated edge did not become healthy within $HealthTimeoutSeconds seconds."
    }
} catch {
    $updateError = $_
    if ($runtimeMutationAttempted) {
        try {
            if ($replacementStarted) {
                Stop-EdgeRuntime -Runtime $runtime -ExecutablePath $exeTarget -TestEventLog $TestRuntimeEventLog -Task $TaskName -ScheduledTaskPath $TaskPath
                if (-not (Test-Path -LiteralPath $backupBinary -PathType Leaf)) {
                    throw "Previous binary backup is unavailable."
                }
                Copy-Item -LiteralPath $backupBinary -Destination $exeTarget -Force
                if ($hadInstalledManifest -and (Test-Path -LiteralPath $backupManifest -PathType Leaf)) {
                    Copy-Item -LiteralPath $backupManifest -Destination $installedManifest -Force
                } elseif (-not $hadInstalledManifest -and (Test-Path -LiteralPath $installedManifest)) {
                    Remove-Item -LiteralPath $installedManifest -Force
                }
            }
            Start-EdgeRuntime -Runtime $runtime -TestEventLog $TestRuntimeEventLog
        } catch {
            throw "Update failed: $($updateError.Exception.Message) Rollback also failed: $($_.Exception.Message)"
        }
    }
    throw $updateError
} finally {
    if (Test-Path -LiteralPath $stagedBinary) { Remove-Item -LiteralPath $stagedBinary -Force }
    if (Test-Path -LiteralPath $stagedManifest) { Remove-Item -LiteralPath $stagedManifest -Force }
}

Write-Host "Updated edge-agent to version $($manifest.version)."
Write-Host "Backup retained at: $backupDir"
Write-Host "Data preserved at: $dataFull"
