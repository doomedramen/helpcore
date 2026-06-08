"use client";

import * as React from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Line,
  LineChart,
  Pie,
  PieChart,
  XAxis,
  YAxis,
} from "recharts";
import { ChartContainer, ChartTooltip, type ChartConfig } from "@/app/components/ui/chart";

const DEFAULT_COLORS = [
  "#3498db",
  "#e74c3c",
  "#2ecc71",
  "#f1c40f",
  "#9b59b6",
  "#1abc9c",
  "#e67e22",
  "#34495e",
];

export interface InlineChartProps {
  chartType: "pie" | "bar" | "line" | "donut";
  labels: string[];
  values: number[];
  colors?: string[];
  width?: number;
  height?: number;
}

function InlineTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: { payload: { label: string; value: number }; value: number }[];
}) {
  if (!active || !payload?.length) return null;
  const item = payload[0];
  return (
    <div className="rounded-lg border border-border/50 bg-background px-2.5 py-1.5 text-xs shadow-xl">
      <div className="text-muted-foreground">{item.payload.label}</div>
      <div className="font-mono font-medium tabular-nums">{item.value.toLocaleString()}</div>
    </div>
  );
}

export default function InlineChart({
  chartType,
  labels,
  values,
  colors,
  width = 400,
  height = 250,
}: InlineChartProps) {
  if (labels.length === 0 || values.length === 0 || labels.length !== values.length) {
    return null;
  }

  const palette = colors?.length ? colors : DEFAULT_COLORS;

  const data = labels.map((label, i) => ({
    label,
    value: values[i],
    fill: palette[i % palette.length],
  }));

  const config: ChartConfig = Object.fromEntries(
    data.map((item) => [item.label, { label: item.label, color: item.fill }]),
  );

  const chart = (() => {
    switch (chartType) {
      case "pie":
      case "donut":
        return (
          <PieChart>
            <Pie
              data={data}
              dataKey="value"
              nameKey="label"
              cx="50%"
              cy="50%"
              innerRadius={chartType === "donut" ? "55%" : undefined}
              outerRadius="80%"
              labelLine={false}
              label={({ payload, percent }) =>
                `${payload.label} ${((percent ?? 0) * 100).toFixed(0)}%`
              }
            >
              {data.map((entry, i) => (
                <Cell key={i} fill={entry.fill} />
              ))}
            </Pie>
            <ChartTooltip content={<InlineTooltip />} />
          </PieChart>
        );
      case "bar":
        return (
          <BarChart data={data}>
            <CartesianGrid vertical={false} />
            <XAxis dataKey="label" tickLine={false} axisLine={false} tickMargin={8} />
            <YAxis tickLine={false} axisLine={false} tickMargin={8} />
            <ChartTooltip content={<InlineTooltip />} />
            <Bar dataKey="value" radius={4}>
              {data.map((entry, i) => (
                <Cell key={i} fill={entry.fill} />
              ))}
            </Bar>
          </BarChart>
        );
      case "line":
        return (
          <LineChart data={data}>
            <CartesianGrid vertical={false} />
            <XAxis dataKey="label" tickLine={false} axisLine={false} tickMargin={8} />
            <YAxis tickLine={false} axisLine={false} tickMargin={8} />
            <ChartTooltip content={<InlineTooltip />} />
            <Line
              dataKey="value"
              stroke={palette[0]}
              strokeWidth={2}
              dot={{ fill: palette[0] }}
              activeDot={{ r: 4 }}
            />
          </LineChart>
        );
      default:
        return null;
    }
  })();

  if (!chart) return null;

  return (
    <ChartContainer config={config} className="w-full" initialDimension={{ width, height }}>
      {chart}
    </ChartContainer>
  );
}
