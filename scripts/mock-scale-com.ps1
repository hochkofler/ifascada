param(
    [string]$Port = "COM9",
    [int]$BaudRate = 9600,
    [int]$DataBits = 8,
    [string]$Parity = "None",
    [string]$StopBits = "One",
    [string]$NewLine = "`r`n",
    [int]$IntervalMs = 700,
    [int]$Count = 20,
    [double]$StartValue = 12.3450,
    [double]$Step = 0.0100,
    [string]$Unit = "g",
    [string]$LogPath = ""
)

$ErrorActionPreference = "Stop"

function New-ScaleFrame([double]$v, [string]$u) {
    $sign = if ($v -ge 0) { "+" } else { "-" }
    $abs = [Math]::Abs($v)
    return "{0} {1:0.0000} {2}" -f $sign, $abs, $u
}

$serial = [System.IO.Ports.SerialPort]::new($Port, $BaudRate)
$serial.DataBits = $DataBits
$serial.NewLine = $NewLine
$serial.Parity = [System.IO.Ports.Parity]::$Parity
$serial.StopBits = [System.IO.Ports.StopBits]::$StopBits
$serial.ReadTimeout = 100
$serial.WriteTimeout = 500

try {
    $serial.Open()
    Write-Host "Mock scale writer connected on $Port"
    if ($LogPath) {
        $dir = Split-Path -Parent $LogPath
        if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
        if (Test-Path $LogPath) { Remove-Item $LogPath -Force }
    }
    for ($i = 0; $i -lt $Count; $i++) {
        $value = $StartValue + ($Step * $i)
        $frame = New-ScaleFrame -v $value -u $Unit
        $ts = [DateTime]::UtcNow.ToString("o")
        $serial.WriteLine($frame)
        Write-Host "TX [$ts] => $frame"
        if ($LogPath) {
            @{ ts = $ts; frame = $frame } | ConvertTo-Json -Compress | Add-Content -Path $LogPath -Encoding UTF8
        }
        Start-Sleep -Milliseconds $IntervalMs
    }
}
finally {
    if ($serial.IsOpen) {
        $serial.Close()
    }
    $serial.Dispose()
    Write-Host "Mock scale writer stopped"
}
