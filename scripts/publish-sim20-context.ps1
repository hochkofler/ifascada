param(
    [string]$Site = "plant-a",
    [string]$MqttHost = "127.0.0.1",
    [int]$MqttPort = 51883,
    [int]$Cycles = 120,
    [int]$IntervalMs = 500
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

function Publish-Mqtt([string]$BrokerHost, [int]$Port, [string]$Topic, [string]$Payload) {
    if (Get-Command mosquitto_pub -ErrorAction SilentlyContinue) {
        $Payload | mosquitto_pub -h $BrokerHost -p $Port -t $Topic -s
        return
    }
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        throw "Missing 'mosquitto_pub' and no 'docker' fallback available."
    }
    $Payload | docker exec -i ifascada-mosquitto mosquitto_pub -h host.docker.internal -p $Port -t $Topic -s | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Failed publishing via docker fallback (ifascada-mosquitto)."
    }
}

$edges = @(
    @{ edge = "edge-pack-1"; prefix = "tag_p1_t" },
    @{ edge = "edge-pack-2"; prefix = "tag_p2_t" },
    @{ edge = "edge-mix-1";  prefix = "tag_m1_t" },
    @{ edge = "edge-mix-2";  prefix = "tag_m2_t" }
)

for ($c = 0; $c -lt $Cycles; $c++) {
    $t = Get-Date
    foreach ($e in $edges) {
        for ($i = 1; $i -le 5; $i++) {
            $tag = "{0}{1}" -f $e.prefix, $i.ToString("000")
            $topic = "scada/$Site/edge/$($e.edge)/telemetry/tag/$tag"
            $base = 10 * $i
            $noise = [Math]::Sin(($c + $i) / 5.0) * 0.9
            $value = [Math]::Round($base + $noise, 3)
            $payload = @{
                schema_version = 1
                source = "sim/$($e.edge)"
                tag_id = $tag
                value = $value
                quality = @{ status = "Good"; reason = $null }
                timestamp = [DateTime]::UtcNow.ToString("o")
            } | ConvertTo-Json -Compress
            Publish-Mqtt -BrokerHost $MqttHost -Port $MqttPort -Topic $topic -Payload $payload
        }
    }
    Start-Sleep -Milliseconds $IntervalMs
}

Write-Host "SIM20 publish completed."
