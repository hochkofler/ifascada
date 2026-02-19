# Fake Printer (TCP Listener) - Improved Version
# Listens on Port 9100 and outputs received text to console.
# Handles disconnections and binary data more gracefully.

$port = 9100
$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Any, $port)
$listener.Start()

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host " 🖨️  FAKE PRINTER (TCP) LISTENING ON $port" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

try {
    while ($true) {
        if ($listener.Pending()) {
            $client = $listener.AcceptTcpClient()
            Write-Host "✅ Connection Accepted from $($client.Client.RemoteEndPoint)" -ForegroundColor Green
            $stream = $client.GetStream()
            $buffer = New-Object byte[] 4096

            while ($client.Connected) {
                if ($stream.DataAvailable) {
                    $count = $stream.Read($buffer, 0, $buffer.Length)
                    if ($count -eq 0) { break } # Disconnected

                    # Decode and handle data
                    $recv_bytes = $buffer[0..($count-1)]
                    
                    Write-Host "`n--- Received Data ($count bytes) ---" -ForegroundColor Yellow
                    
                    # 1. Printable Text View
                    $clean_text = [string]::Join("", ($recv_bytes | ForEach-Object { 
                        if ($_ -ge 32 -and $_ -le 126) { [char]$_ } 
                        elseif ($_ -eq 10) { "`n" }
                        elseif ($_ -eq 13) { "" } # Ignore CR
                        else { "·" } 
                    }))
                    Write-Host $clean_text

                    # 2. Hex View (for debugging ESC/POS codes)
                    $hex = [System.BitConverter]::ToString($recv_bytes)
                    Write-Host "Hex: $hex" -ForegroundColor Gray
                    Write-Host "----------------------------------`n" -ForegroundColor Yellow
                }
                if (-not $client.Client.Connected) { break }
                Start-Sleep -Milliseconds 50
            }
            Write-Host "❌ Connection Closed" -ForegroundColor Red
            $client.Close()
        }
        Start-Sleep -Milliseconds 100
    }
}
catch {
    Write-Error $_
}
finally {
    $listener.Stop()
}
