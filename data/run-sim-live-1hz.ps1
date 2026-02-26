$ErrorActionPreference = "Stop"
$site = "plant-a"
$edge = "edge-com-01"
$tag = "tag_scale_manual_compound"
$topic = "scada/$site/edge/$edge/telemetry/tag/$tag"
for ($i = 0; $i -lt 600; $i++) {
  $v = [Math]::Round((15.0 + [Math]::Sin($i / 8.0) * 0.8), 3)
  $payload = @{
    schema_version = 1
    source = "sim/edge-com-01"
    tag_id = $tag
    value = @{ value = $v; unit = "g"; raw = "$v g" }
    quality = @{ status = "Good"; reason = $null }
    timestamp = [DateTime]::UtcNow.ToString("o")
  } | ConvertTo-Json -Compress -Depth 6
  $payload | docker exec -i ifascada-mosquitto mosquitto_pub -h host.docker.internal -p 51883 -t $topic -s | Out-Null
  Start-Sleep -Milliseconds 1000
}
Write-Host "done"
