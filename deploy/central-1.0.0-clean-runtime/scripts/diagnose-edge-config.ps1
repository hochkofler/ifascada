param(
    [string]$BaseUrl = "http://127.0.0.1:8088",
    [string]$EdgeId = "edge-com-01",
    [string]$EnrollToken = "dev-edge-enroll-token",
    [string]$ComposeFile = ".\docker-compose.yml",
    [string]$CentralContainer = "ifascada-central-server",
    [string]$DbContainer = "ifascada-timescaledb",
    [switch]$ShowCentralLogs
)

$ErrorActionPreference = "Stop"

function Step([string]$Title) {
    Write-Host ""
    Write-Host "== $Title =="
}

function Try-InvokeJson([string]$Method, [string]$Url, [object]$Body = $null) {
    try {
        if ($null -eq $Body) {
            return Invoke-RestMethod -Method $Method -Uri $Url -UseBasicParsing -TimeoutSec 10
        }
        $json = $Body | ConvertTo-Json -Depth 8
        return Invoke-RestMethod -Method $Method -Uri $Url -ContentType "application/json" -Body $json -UseBasicParsing -TimeoutSec 15
    } catch {
        Write-Host "Request failed: $Method $Url"
        Write-Host $_.Exception.Message
        return $null
    }
}

Step "API health"
$health = Try-InvokeJson -Method "GET" -Url "$BaseUrl/health/live"
if ($null -ne $health) { $health | ConvertTo-Json -Depth 5 }

Step "edge config check"
$checkBody = @{
    edge_id = $EdgeId
    enrollment_token = $EnrollToken
    current_config_hash = $null
}
$check = Try-InvokeJson -Method "POST" -Url "$BaseUrl/api/edge/config/check" -Body $checkBody
if ($null -ne $check) { $check | ConvertTo-Json -Depth 8 }

Step "edge runtime config envelope"
$runtimeUrl = "$BaseUrl/api/edge/config/runtime?edge_id=$EdgeId"
$runtime = Try-InvokeJson -Method "GET" -Url $runtimeUrl
if ($null -ne $runtime) {
    [PSCustomObject]@{
        edge_id = $runtime.edge_id
        key_id = $runtime.key_id
        algorithm = $runtime.algorithm
        config_hash = $runtime.config_hash
        payload_json_length = if ($null -ne $runtime.payload_json) { $runtime.payload_json.Length } else { 0 }
        signature_hex_length = if ($null -ne $runtime.signature_hex) { $runtime.signature_hex.Length } else { 0 }
    } | Format-List
}

Step "central edge env vars"
docker exec $CentralContainer sh -lc "printenv | grep CENTRAL_EDGE || true"

Step "central runtime config file"
docker exec $CentralContainer sh -lc "ls -l /app/config/bootstrap.example.json || true"

Step "db edges"
docker exec $DbContainer psql -U postgres -d rustscada -c "SELECT edge_code, status FROM edges ORDER BY edge_code;"

Step "db connections for edge"
docker exec $DbContainer psql -U postgres -d rustscada -c "SELECT e.edge_code, c.connection_code, c.driver_type FROM connections c JOIN edges e ON e.id = c.edge_id WHERE e.edge_code = '$EdgeId' ORDER BY c.connection_code;"

if ($ShowCentralLogs) {
    Step "central logs (tail 200)"
    docker logs --tail 200 $CentralContainer
}

Write-Host ""
Write-Host "Done."
