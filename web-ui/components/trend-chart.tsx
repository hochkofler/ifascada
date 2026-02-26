"use client";

import { memo, useMemo } from "react";
import dynamic from "next/dynamic";
import { TagHistory } from "@/lib/api";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

export const TrendChart = memo(function TrendChart({ data }: { data: TagHistory[] }) {
  const toNumeric = (v: unknown): number => {
    if (typeof v === "number") return v;
    if (typeof v === "string") {
      const asNum = Number.parseFloat(v);
      if (!Number.isNaN(asNum)) return asNum;
      try {
        const parsed = JSON.parse(v) as { value?: unknown };
        if (typeof parsed?.value === "number") return parsed.value;
      } catch {}
      return 0;
    }
    if (v && typeof v === "object") {
      const rec = v as Record<string, unknown>;
      if (typeof rec.value === "number") return rec.value;
      for (const x of Object.values(rec)) {
        if (typeof x === "number") return x;
      }
    }
    return 0;
  };

  const points = useMemo(() => [...data].reverse(), [data]);
  const option = useMemo(() => ({
    backgroundColor: "transparent",
    animation: false,
    animationDuration: 0,
    animationDurationUpdate: 0,
    tooltip: { trigger: "axis" },
    xAxis: {
      type: "category",
      data: points.map((p) => new Date(p.ts).toLocaleTimeString()),
      axisLine: { lineStyle: { color: "#37505f" } },
      axisLabel: { color: "#91a4b2" },
    },
    yAxis: {
      type: "value",
      axisLine: { lineStyle: { color: "#37505f" } },
      splitLine: { lineStyle: { color: "#22313c" } },
      axisLabel: { color: "#91a4b2" },
    },
    series: [
      {
        type: "line",
        smooth: false,
        showSymbol: false,
        lineStyle: { width: 2, color: "#23b5d3" },
        areaStyle: { color: "rgba(35,181,211,0.18)" },
        data: points.map((p) => toNumeric(p.value)),
      },
    ],
  }), [points]);

  return <ReactECharts option={option} notMerge={true} lazyUpdate={false} style={{ height: 320 }} />;
});
