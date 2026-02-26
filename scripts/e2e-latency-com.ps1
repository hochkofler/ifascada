param(
    [string]$ApiBase = "http://127.0.0.1:8088",
    [string]$PgDsn = "host=127.0.0.1 port=55432 user=postgres dbname=rustscada sslmode=disable",
    [string]$Site = "plant-a",
    [string]$Edge = "edge-com-01",
    [string]$Tag = "tag_scale_manual_compound",
    [string]$WriteComPort = "COM8",
    [int]$Samples = 15,
    [int]$IntervalMs = 1000,
    [double]$StartValue = 23.1000,
    [double]$Step = 0.0110,
    [string]$Unit = "g"
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command '$Name'. Install it and retry."
    }
}

function To-EpochMs([datetime]$dt) {
    return [long]([DateTimeOffset]$dt).ToUnixTimeMilliseconds()
}

function Percentile([double[]]$arr, [double]$p) {
    if (-not $arr -or $arr.Count -eq 0) { return $null }
    $sorted = $arr | Sort-Object
    $idx = [Math]::Ceiling(($p / 100.0) * $sorted.Count) - 1
    if ($idx -lt 0) { $idx = 0 }
    if ($idx -ge $sorted.Count) { $idx = $sorted.Count - 1 }
    return [double]$sorted[$idx]
}

Require-Command "python"
Require-Command "psql"

$outDir = "data/e2e-latency"
if (-not (Test-Path $outDir)) { New-Item -ItemType Directory -Path $outDir | Out-Null }

$runId = Get-Date -Format "yyyyMMdd-HHmmss"
$sendLog = Join-Path $outDir "send-$runId.jsonl"
$sseLog = Join-Path $outDir "sse-$runId.jsonl"
$report = Join-Path $outDir "report-$runId.json"
$ssePy = Join-Path $outDir "sse-listener-$runId.py"

$startUtc = [DateTime]::UtcNow.AddSeconds(-2)
$startIso = $startUtc.ToString("o")

@"
import json
import time
import urllib.parse
import urllib.request
import sys

api_base = sys.argv[1].rstrip("/")
site = sys.argv[2]
edge = sys.argv[3]
tag = sys.argv[4]
out_path = sys.argv[5]
max_events = int(sys.argv[6])

qs = urllib.parse.urlencode({
    "site": site,
    "edge": edge,
    "tag": tag,
    "exclude_raw": "true",
    "replay": "false",
})
url = f"{api_base}/api/stream/events?{qs}"

count = 0
with urllib.request.urlopen(url, timeout=120) as resp, open(out_path, "w", encoding="utf-8") as out:
    for raw in resp:
        line = raw.decode("utf-8", errors="ignore").strip()
        if not line.startswith("data: "):
            continue
        try:
            payload = json.loads(line[6:])
        except Exception:
            continue
        p = payload.get("payload") or {}
        value = p.get("value")
        raw_value = None
        if isinstance(value, dict):
            raw_value = value.get("raw")
        elif isinstance(value, str):
            # value can be a JSON-encoded object string
            vtxt = value.strip()
            if vtxt.startswith("{") and vtxt.endswith("}"):
                try:
                    parsed = json.loads(vtxt)
                    if isinstance(parsed, dict) and parsed.get("raw"):
                        raw_value = parsed.get("raw")
                    else:
                        raw_value = value
                except Exception:
                    raw_value = value
            else:
                raw_value = value
        row = {
            "recv_ts_ms": int(time.time() * 1000),
            "published_at": payload.get("published_at"),
            "payload_ts": p.get("timestamp"),
            "raw": raw_value,
            "tag_id": p.get("tag_id"),
        }
        out.write(json.dumps(row, ensure_ascii=False) + "\n")
        out.flush()
        count += 1
        if count >= max_events:
            break
"@ | Set-Content -Encoding UTF8 $ssePy

Write-Host "Starting SSE listener ..."
$sseProc = Start-Process -FilePath python -ArgumentList @($ssePy, $ApiBase, $Site, $Edge, $Tag, $sseLog, "$($Samples + 8)") -PassThru
Start-Sleep -Milliseconds 500

Write-Host "Sending $Samples samples to $WriteComPort ..."
powershell -ExecutionPolicy Bypass -File scripts/mock-scale-com.ps1 `
  -Port $WriteComPort `
  -Count $Samples `
  -IntervalMs $IntervalMs `
  -StartValue $StartValue `
  -Step $Step `
  -Unit $Unit `
  -LogPath $sendLog | Out-Host

Start-Sleep -Seconds 3
if ($sseProc -and -not $sseProc.HasExited) {
    try { Stop-Process -Id $sseProc.Id -Force } catch {}
}

if (-not (Test-Path $sendLog)) { throw "send log not found: $sendLog" }
if (-not (Test-Path $sseLog)) { Write-Warning "sse log not found: $sseLog" }

Write-Host "Querying DB telemetry ..."
$sql = @"
WITH src AS (
  SELECT
    id,
    ts,
    (payload_json->>'timestamp')::timestamptz AS payload_ts,
    CASE
      WHEN jsonb_typeof(payload_json->'value') = 'object' THEN payload_json->'value'->>'raw'
      WHEN jsonb_typeof(payload_json->'value') = 'string'
           AND left(payload_json->>'value', 1) = '{'
        THEN ((payload_json->>'value')::jsonb->>'raw')
      WHEN jsonb_typeof(payload_json->'value') = 'string' THEN payload_json->>'value'
      ELSE NULL
    END AS raw_value
  FROM telemetry_ingest_events
  WHERE site_code = '$Site'
    AND edge_code = '$Edge'
    AND tag_code = '$Tag'
    AND ts >= '$startIso'::timestamptz
  ORDER BY id DESC
  LIMIT 500
)
SELECT id, EXTRACT(EPOCH FROM ts) * 1000 AS ts_ms, EXTRACT(EPOCH FROM payload_ts) * 1000 AS payload_ts_ms, raw_value
FROM src
WHERE raw_value IS NOT NULL;
"@
$dbRowsRaw = psql "$PgDsn" -At -F "`t" -c $sql

$sendRows = Get-Content $sendLog | ForEach-Object { $_ | ConvertFrom-Json }
$sseRows = @()
if (Test-Path $sseLog) {
    $rawSse = Get-Content $sseLog -Raw
    if ($rawSse) {
        $lines = $rawSse -split "`r?`n"
        foreach ($line in $lines) {
            if (-not $line) { continue }
            try {
                $sseRows += ($line | ConvertFrom-Json)
            } catch {
                # Backward compatibility for malformed files with literal '\n'
                foreach ($chunk in ($line -split "\\n")) {
                    if (-not $chunk) { continue }
                    try { $sseRows += ($chunk | ConvertFrom-Json) } catch {}
                }
            }
        }
    }
}

function Normalize-Raw([string]$raw) {
    if (-not $raw) { return $null }
    $txt = $raw.Trim()
    if ($txt.StartsWith("{") -and $txt.EndsWith("}")) {
        try {
            $obj = $txt | ConvertFrom-Json
            if ($obj.raw) { return [string]$obj.raw }
        } catch {}
    }
    return $txt
}

$dbByRaw = @{}
foreach ($line in $dbRowsRaw) {
    $parts = $line -split "`t", 4
    if ($parts.Count -lt 4) { continue }
    $raw = Normalize-Raw ([string]$parts[3])
    if (-not $raw) { continue }
    if (-not $dbByRaw.ContainsKey($raw)) { $dbByRaw[$raw] = @() }
    $dbByRaw[$raw] += [pscustomobject]@{
        id = [long]$parts[0]
        ts_ms = [double]$parts[1]
        payload_ts_ms = [double]$parts[2]
        raw = $raw
    }
}

$sseByRaw = @{}
foreach ($r in $sseRows) {
    $raw = Normalize-Raw ([string]$r.raw)
    if (-not $raw) { continue }
    if (-not $sseByRaw.ContainsKey($raw)) { $sseByRaw[$raw] = @() }
    $sseByRaw[$raw] += [double]$r.recv_ts_ms
}

$pairs = @()
foreach ($s in $sendRows) {
    $sendMs = To-EpochMs ([DateTime]::Parse([string]$s.ts))
    $raw = Normalize-Raw ([string]$s.frame)
    $db = $null
    if ($dbByRaw.ContainsKey($raw) -and $dbByRaw[$raw].Count -gt 0) {
        $db = $dbByRaw[$raw][0]
    }
    $sseMs = $null
    if ($sseByRaw.ContainsKey($raw) -and $sseByRaw[$raw].Count -gt 0) {
        $sseMs = [double]$sseByRaw[$raw][0]
    }
    $pairs += [pscustomobject]@{
        raw = $raw
        send_ts_ms = [double]$sendMs
        payload_ts_ms = if ($db) { [double]$db.payload_ts_ms } else { $null }
        db_ts_ms = if ($db) { [double]$db.ts_ms } else { $null }
        sse_recv_ts_ms = $sseMs
        send_to_payload_ms = if ($db) { [double]$db.payload_ts_ms - [double]$sendMs } else { $null }
        send_to_db_ms = if ($db) { [double]$db.ts_ms - [double]$sendMs } else { $null }
        send_to_sse_ms = if ($sseMs) { [double]$sseMs - [double]$sendMs } else { $null }
    }
}

$latDb = @($pairs | Where-Object { $_.send_to_db_ms -ne $null } | ForEach-Object { [double]$_.send_to_db_ms })
$latSse = @($pairs | Where-Object { $_.send_to_sse_ms -ne $null } | ForEach-Object { [double]$_.send_to_sse_ms })
$latPayload = @($pairs | Where-Object { $_.send_to_payload_ms -ne $null } | ForEach-Object { [double]$_.send_to_payload_ms })

$summary = [pscustomobject]@{
    run_id = $runId
    samples_sent = $sendRows.Count
    matched_db = $latDb.Count
    matched_sse = $latSse.Count
    send_to_payload_ms = [pscustomobject]@{
        p50 = Percentile $latPayload 50
        p95 = Percentile $latPayload 95
        max = if ($latPayload.Count -gt 0) { ($latPayload | Measure-Object -Maximum).Maximum } else { $null }
    }
    send_to_db_ms = [pscustomobject]@{
        p50 = Percentile $latDb 50
        p95 = Percentile $latDb 95
        max = if ($latDb.Count -gt 0) { ($latDb | Measure-Object -Maximum).Maximum } else { $null }
    }
    send_to_sse_ms = [pscustomobject]@{
        p50 = Percentile $latSse 50
        p95 = Percentile $latSse 95
        max = if ($latSse.Count -gt 0) { ($latSse | Measure-Object -Maximum).Maximum } else { $null }
    }
}

@{
    summary = $summary
    details = $pairs
} | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 $report

Write-Host ""
Write-Host "===== LATENCY SUMMARY ====="
$summary | ConvertTo-Json -Depth 5
Write-Host ""
Write-Host "Report: $report"
Write-Host "Send log: $sendLog"
Write-Host "SSE log:  $sseLog"
