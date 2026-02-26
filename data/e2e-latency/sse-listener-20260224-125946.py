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
            raw_value = value
        row = {
            "recv_ts_ms": int(time.time() * 1000),
            "published_at": payload.get("published_at"),
            "payload_ts": p.get("timestamp"),
            "raw": raw_value,
            "tag_id": p.get("tag_id"),
        }
        out.write(json.dumps(row, ensure_ascii=False) + "\\n")
        out.flush()
        count += 1
        if count >= max_events:
            break
