$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$libPath = Join-Path (Split-Path -Parent $here) "lib\DeployDockerService.ps1"
. $libPath

Describe "Get-CurrentImageTag" {
    It "extracts the value of the named variable" {
        $env = "RUST_LOG=info`r`nCENTRAL_IMAGE=ifascada/central-server:1.0.2`r`nCENTRAL_API_PORT=8088"
        Get-CurrentImageTag -EnvContent $env -VarName "CENTRAL_IMAGE" | Should Be "ifascada/central-server:1.0.2"
    }

    It "throws when the variable is not present" {
        $env = "RUST_LOG=info"
        $thrown = $false
        try {
            Get-CurrentImageTag -EnvContent $env -VarName "CENTRAL_IMAGE"
        } catch {
            $thrown = $true
        }
        $thrown | Should Be $true
    }
}

Describe "Set-ImageTag" {
    It "replaces the value of the named variable, leaving other lines untouched" {
        $env = "RUST_LOG=info`r`nCENTRAL_IMAGE=ifascada/central-server:1.0.2`r`nCENTRAL_API_PORT=8088"
        $result = Set-ImageTag -EnvContent $env -VarName "CENTRAL_IMAGE" -NewValue "ifascada/central-server:1.0.3"
        $result | Should Match "CENTRAL_IMAGE=ifascada/central-server:1.0.3"
        $result | Should Match "RUST_LOG=info"
        $result | Should Match "CENTRAL_API_PORT=8088"
    }

    It "throws instead of appending when the variable is not present" {
        $env = "RUST_LOG=info"
        $thrown = $false
        try {
            Set-ImageTag -EnvContent $env -VarName "CENTRAL_IMAGE" -NewValue "x"
        } catch {
            $thrown = $true
        }
        $thrown | Should Be $true
    }
}

Describe "Test-ServiceHealthy" {
    BeforeEach {
        Mock Start-Sleep {}
    }

    It "returns true immediately when the first check succeeds" {
        Mock Invoke-WebRequest { [pscustomobject]@{ StatusCode = 200 } }
        Test-ServiceHealthy -Url "http://example.invalid/health" -MaxAttempts 5 -PollIntervalSeconds 1 | Should Be $true
        Assert-MockCalled Invoke-WebRequest -Times 1 -Exactly
    }

    It "retries after a failed attempt and succeeds on the next one" {
        $script:callCount = 0
        Mock Invoke-WebRequest {
            $script:callCount++
            if ($script:callCount -lt 3) { throw "connection refused" }
            [pscustomobject]@{ StatusCode = 200 }
        }
        Test-ServiceHealthy -Url "http://example.invalid/health" -MaxAttempts 5 -PollIntervalSeconds 1 | Should Be $true
        $script:callCount | Should Be 3
    }

    It "returns false after exhausting all attempts" {
        $script:callCount3 = 0
        Mock Invoke-WebRequest {
            $script:callCount3++
            throw "connection refused"
        }
        Test-ServiceHealthy -Url "http://example.invalid/health" -MaxAttempts 3 -PollIntervalSeconds 1 | Should Be $false
        $script:callCount3 | Should Be 3
    }
}

Describe "Invoke-RemoteCommand" {
    It "returns captured output on success" {
        Mock Invoke-Ssh { "remote output line" ; $global:LASTEXITCODE = 0 }
        $result = Invoke-RemoteCommand -TargetHost "192.168.103.154" -SshUser "ifa" -SshKeyPath "C:\key" -Command "echo hi"
        $result | Should Be "remote output line"
    }

    It "throws when the remote command fails" {
        Mock Invoke-Ssh { "some error"; $global:LASTEXITCODE = 1 }
        $thrown = $false
        try {
            Invoke-RemoteCommand -TargetHost "192.168.103.154" -SshUser "ifa" -SshKeyPath "C:\key" -Command "false"
        } catch {
            $thrown = $true
        }
        $thrown | Should Be $true
    }
}

Describe "Copy-ToRemote" {
    It "does not throw on a successful copy" {
        Mock Invoke-Scp { $global:LASTEXITCODE = 0 }
        Copy-ToRemote -TargetHost "192.168.103.154" -SshUser "ifa" -SshKeyPath "C:\key" -LocalPath "C:\local.tar" -RemotePath "C:/remote.tar"
    }

    It "throws when the copy fails" {
        Mock Invoke-Scp { $global:LASTEXITCODE = 1 }
        $thrown = $false
        try {
            Copy-ToRemote -TargetHost "192.168.103.154" -SshUser "ifa" -SshKeyPath "C:\key" -LocalPath "C:\local.tar" -RemotePath "C:/remote.tar"
        } catch {
            $thrown = $true
        }
        $thrown | Should Be $true
    }
}
