$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$packageRoot = Split-Path -Parent $here
$updater = Join-Path $packageRoot "scripts\update-edge.ps1"
$builder = Join-Path $packageRoot "scripts\build-edge-package.ps1"

function New-UpdateFixture {
    param(
        [string]$Root,
        [string]$IncomingContent = "new-binary",
        [string]$DeclaredHash,
        [switch]$JunctionPackageBin
    )

    $fixturePackage = Join-Path $Root "package"
    $fixtureBin = Join-Path $fixturePackage "bin"
    $fixtureInstall = Join-Path $Root "install"
    $fixtureData = Join-Path $Root "data"
    New-Item -ItemType Directory -Force -Path $fixturePackage, $fixtureInstall, $fixtureData | Out-Null
    if ($JunctionPackageBin) {
        $realBin = Join-Path $Root "real-package-bin"
        New-Item -ItemType Directory -Force -Path $realBin | Out-Null
        New-Item -ItemType Junction -Path $fixtureBin -Target $realBin | Out-Null
    } else {
        New-Item -ItemType Directory -Force -Path $fixtureBin | Out-Null
    }

    $incoming = Join-Path $fixtureBin "edge-agent.exe"
    $installed = Join-Path $fixtureInstall "edge-agent.exe"
    $dataFile = Join-Path $fixtureData "edge.env"
    [IO.File]::WriteAllText($incoming, $IncomingContent)
    [IO.File]::WriteAllText($installed, "old-binary")
    [IO.File]::WriteAllText($dataFile, "EDGE_AGENT=edge-test`r`nSECRET=preserve-me")

    $actualHash = (Get-FileHash -Path $incoming -Algorithm SHA256).Hash.ToLowerInvariant()
    $hash = if ($DeclaredHash) { $DeclaredHash } else { $actualHash }
    @{
        manifest_version = 1
        version = "1.1.0"
        config_schema_version = 1
        minimum_central_version = "1.0.0"
        binary = @{
            path = "bin/edge-agent.exe"
            sha256 = $hash
        }
    } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $fixturePackage "release-manifest.json") -Encoding UTF8

    @{
        manifest_version = 1
        version = "1.0.0"
        config_schema_version = 1
        minimum_central_version = "1.0.0"
        binary = @{
            path = "edge-agent.exe"
            sha256 = (Get-FileHash -Path $installed -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $fixtureInstall "release-manifest.json") -Encoding UTF8

    return @{
        Package = $fixturePackage
        Install = $fixtureInstall
        Data = $fixtureData
        Installed = $installed
        DataFile = $dataFile
    }
}

Describe "build-edge-package.ps1 manifest" {
    It "copies a supplied binary and writes its real SHA-256" {
        $source = Join-Path $TestDrive "build\edge-agent.exe"
        $output = Join-Path $TestDrive "build\package"
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $source) | Out-Null
        [IO.File]::WriteAllText($source, "release-binary")

        & $builder -BinaryPath $source -OutputRoot $output -Version "1.1.0" -ConfigSchemaVersion 2 -MinimumCentralVersion "1.0.0"

        $packagedBinary = Join-Path $output "bin\edge-agent.exe"
        $manifest = Get-Content (Join-Path $output "release-manifest.json") -Raw | ConvertFrom-Json
        [IO.File]::ReadAllText($packagedBinary) | Should Be "release-binary"
        $manifest.version | Should Be "1.1.0"
        $manifest.config_schema_version | Should Be 2
        $manifest.binary.path | Should Be "bin/edge-agent.exe"
        $manifest.binary.sha256 | Should Be (Get-FileHash $packagedBinary -Algorithm SHA256).Hash.ToLowerInvariant()
        Test-Path (Join-Path $output "scripts\update-edge.ps1") | Should Be $true
    }

    It "rejects a version that the updater cannot safely consume" {
        $source = Join-Path $TestDrive "invalid-build\edge-agent.exe"
        $output = Join-Path $TestDrive "invalid-build\package"
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $source) | Out-Null
        [IO.File]::WriteAllText($source, "release-binary")
        $thrown = $false

        try {
            & $builder -BinaryPath $source -OutputRoot $output -Version ".."
        } catch { $thrown = $true }

        $thrown | Should Be $true
        Test-Path (Join-Path $output "release-manifest.json") | Should Be $false
    }
}

Describe "update-edge.ps1 transaction" {
    BeforeEach { $env:IFASCADA_UPDATER_TEST_MODE = "1" }
    AfterEach { Remove-Item Env:IFASCADA_UPDATER_TEST_MODE -ErrorAction SilentlyContinue }

    It "rejects a hash mismatch before changing the installation" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "hash") -DeclaredHash ("0" * 64)

        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -HealthCheckScript { $true }
        } catch {
            $thrown = $true
        }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
        Test-Path (Join-Path $fixture.Install "releases") | Should Be $false
    }

    It "backs up and replaces only the binary while preserving DataRoot" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "success")
        $dataHashBefore = (Get-FileHash -Path $fixture.DataFile -Algorithm SHA256).Hash

        & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -HealthCheckScript { $true }

        [IO.File]::ReadAllText($fixture.Installed) | Should Be "new-binary"
        $snapshot = Get-ChildItem (Join-Path $fixture.Install "releases\1.0.0") -Directory | Select-Object -First 1
        [IO.File]::ReadAllText((Join-Path $snapshot.FullName "edge-agent.exe")) | Should Be "old-binary"
        (Get-FileHash -Path $fixture.DataFile -Algorithm SHA256).Hash | Should Be $dataHashBefore
        (Get-Content (Join-Path $fixture.Install "release-manifest.json") -Raw | ConvertFrom-Json).version | Should Be "1.1.0"
    }

    It "restores the previous binary when the health check fails" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "rollback")

        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -HealthTimeoutSeconds 0 -HealthCheckScript { $false }
        } catch {
            $thrown = $true
        }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
        (Get-Content (Join-Path $fixture.Install "release-manifest.json") -Raw | ConvertFrom-Json).version | Should Be "1.0.0"
        $snapshot = Get-ChildItem (Join-Path $fixture.Install "releases\1.0.0") -Directory | Select-Object -First 1
        [IO.File]::ReadAllText((Join-Path $snapshot.FullName "edge-agent.exe")) | Should Be "old-binary"
    }

    It "restarts the unchanged runtime when backup creation fails after stop" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "backup-failure")
        $runtimeLog = Join-Path $TestDrive "backup-failure\runtime-events.log"
        [IO.File]::WriteAllText((Join-Path $fixture.Install "releases"), "blocks-directory-creation")

        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -TestRuntimeEventLog $runtimeLog -HealthCheckScript { $true }
        } catch {
            $thrown = $true
        }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
        @(Get-Content $runtimeLog) | Should Be @("stop", "start")
    }

    It "rejects wildcard runtime names before mutation" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "wildcard")
        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -ServiceName "*" -HealthCheckScript { $true }
        } catch { $thrown = $true }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
    }

    It "rejects an installed traversal version before stopping" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "old-traversal")
        $oldManifestPath = Join-Path $fixture.Install "release-manifest.json"
        $oldManifest = Get-Content $oldManifestPath -Raw | ConvertFrom-Json
        $oldManifest.version = ".."
        $oldManifest | ConvertTo-Json -Depth 5 | Set-Content $oldManifestPath -Encoding UTF8

        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -HealthCheckScript { $true }
        } catch { $thrown = $true }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
    }

    It "rejects malformed manifest scalar types and schema zero" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "manifest-types")
        $manifestPath = Join-Path $fixture.Package "release-manifest.json"
        $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
        $manifest.manifest_version = $true
        $manifest.config_schema_version = 0
        $manifest | ConvertTo-Json -Depth 5 | Set-Content $manifestPath -Encoding UTF8

        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -HealthCheckScript { $true }
        } catch { $thrown = $true }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
    }

    It "rejects schema zero even when other manifest types are valid" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "schema-zero")
        $manifestPath = Join-Path $fixture.Package "release-manifest.json"
        $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
        $manifest.config_schema_version = 0
        $manifest | ConvertTo-Json -Depth 5 | Set-Content $manifestPath -Encoding UTF8

        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -HealthCheckScript { $true }
        } catch { $thrown = $true }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
    }

    It "requires the contracted binary path" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "binary-path")
        $manifestPath = Join-Path $fixture.Package "release-manifest.json"
        $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
        $manifest.binary.path = "bin/../bin/edge-agent.exe"
        $manifest | ConvertTo-Json -Depth 5 | Set-Content $manifestPath -Encoding UTF8

        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -HealthCheckScript { $true }
        } catch { $thrown = $true }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
    }

    It "rejects overlapping install and data roots" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "overlap")
        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Install -RuntimeMode test -HealthCheckScript { $true }
        } catch { $thrown = $true }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
    }

    It "rejects a package binary reached through a child junction" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "package-junction") -JunctionPackageBin
        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -HealthCheckScript { $true }
        } catch { $thrown = $true }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
        Test-Path (Join-Path $fixture.Install "releases") | Should Be $false
    }

    It "does not allow test mode without an explicit test environment gate" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "test-gate")
        Remove-Item Env:IFASCADA_UPDATER_TEST_MODE -ErrorAction SilentlyContinue
        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode test -HealthCheckScript { $true }
        } catch { $thrown = $true }

        $thrown | Should Be $true
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
    }

    It "selects and controls only the exact scheduled task path" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "exact-task")
        $targetTask = New-CimInstance -Namespace Root/Microsoft/Windows/TaskScheduler -ClassName MSFT_ScheduledTask -ClientOnly -Property @{ TaskName = "ifascada-edge"; TaskPath = "\"; State = 3 }
        $otherTask = New-CimInstance -Namespace Root/Microsoft/Windows/TaskScheduler -ClassName MSFT_ScheduledTask -ClientOnly -Property @{ TaskName = "ifascada-edge"; TaskPath = "\Other\"; State = 3 }
        Mock Get-Service { @() }
        Mock Get-ScheduledTask { @($otherTask, $targetTask) }
        Mock Stop-ScheduledTask { }
        Mock Start-ScheduledTask { }
        Mock Get-CimInstance { @() }

        & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode task -TaskName "ifascada-edge" -TaskPath "\" -TestSkipAdminCheck -HealthCheckScript { $true }

        Assert-MockCalled Stop-ScheduledTask -Times 1 -ParameterFilter { $InputObject.TaskPath -eq "\" }
        Assert-MockCalled Start-ScheduledTask -Times 1 -ParameterFilter { $InputObject.TaskPath -eq "\" }
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "new-binary"
    }

    It "propagates a scheduled-task stop failure without replacing the binary" {
        $fixture = New-UpdateFixture -Root (Join-Path $TestDrive "task-stop-failure")
        $targetTask = New-CimInstance -Namespace Root/Microsoft/Windows/TaskScheduler -ClassName MSFT_ScheduledTask -ClientOnly -Property @{ TaskName = "ifascada-edge"; TaskPath = "\"; State = 4 }
        Mock Get-Service { @() }
        Mock Get-ScheduledTask { @($targetTask) }
        Mock Stop-ScheduledTask { throw "scheduled task stop failed" }
        Mock Start-ScheduledTask { }
        Mock Get-CimInstance { @() }

        $thrown = $false
        try {
            & $updater -PackageRoot $fixture.Package -InstallRoot $fixture.Install -DataRoot $fixture.Data -RuntimeMode task -TaskName "ifascada-edge" -TaskPath "\" -TestSkipAdminCheck -HealthCheckScript { $true }
        } catch { $thrown = $true }

        $thrown | Should Be $true
        Assert-MockCalled Stop-ScheduledTask -Times 1
        Assert-MockCalled Start-ScheduledTask -Times 1
        [IO.File]::ReadAllText($fixture.Installed) | Should Be "old-binary"
        Test-Path (Join-Path $fixture.Install "releases") | Should Be $false
    }
}
