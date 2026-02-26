"use client";

import { TagCurrent } from "@/lib/api";
import { formatProcessValue } from "@/lib/hmi-value";

export function PvTile({
  tag,
  selected,
  onSelect,
}: {
  tag: TagCurrent;
  selected?: boolean;
  onSelect?: () => void;
}) {
  const q = String(tag.quality?.status || "Unknown");
  const good = q.toLowerCase() === "good";
  return (
    <button className={`pv ${selected ? "active" : ""}`} onClick={onSelect}>
      <div className="pv-head">
        <span className="pv-tag mono" title={tag.tag_code}>{tag.tag_code}</span>
        <span className={`lamp ${good ? "on" : "off"}`} />
      </div>
      <div className="pv-value mono" title={formatProcessValue(tag.value)}>{formatProcessValue(tag.value)}</div>
      <div className="pv-meta">
        <span className="mono">{tag.device_code}</span>
        <span>{q}</span>
      </div>
    </button>
  );
}
