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
