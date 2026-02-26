"use client";

import dynamic from "next/dynamic";

type Point = { ts: string; value: number; accum: number };

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

export function TrendAccumChart({ data }: { data: Point[] }) {
  const option = {
    backgroundColor: "transparent",
    animation: false,
    tooltip: { trigger: "axis" },
    legend: {
      data: ["Value", "Accumulated"],
      textStyle: { color: "#91a4b2" },
      top: 0,
    },
    grid: { left: 44, right: 24, top: 34, bottom: 30 },
    xAxis: {
      type: "category",
      data: data.map((p) => new Date(p.ts).toLocaleTimeString()),
      axisLine: { lineStyle: { color: "#37505f" } },
      axisLabel: { color: "#91a4b2" },
    },
    yAxis: [
      {
        type: "value",
        axisLine: { lineStyle: { color: "#37505f" } },
        splitLine: { lineStyle: { color: "#22313c" } },
        axisLabel: { color: "#91a4b2" },
      },
      {
        type: "value",
        axisLine: { lineStyle: { color: "#37505f" } },
        splitLine: { show: false },
        axisLabel: { color: "#91a4b2" },
      },
    ],
    series: [
      {
        name: "Value",
        type: "line",
        yAxisIndex: 0,
        showSymbol: false,
        lineStyle: { width: 2, color: "#23b5d3" },
        data: data.map((p) => p.value),
      },
      {
        name: "Accumulated",
        type: "line",
        yAxisIndex: 1,
        showSymbol: false,
        lineStyle: { width: 2, color: "#20d48a" },
        areaStyle: { color: "rgba(32,212,138,0.12)" },
        data: data.map((p) => p.accum),
      },
    ],
  };
  return <ReactECharts option={option} style={{ height: 360 }} />;
}
