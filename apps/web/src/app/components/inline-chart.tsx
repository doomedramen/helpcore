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
import type { RendererProps } from "./content-renderers";

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

const VALID_TYPES = new Set(["pie", "bar", "line", "donut"]);

interface ChartData {
  title?: string;
  labels?: string[];
  values?: number[];
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

function ErrorBox({ message }: { message: string }) {
  return (
    <div className="rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs">
      <span className="font-semibold text-destructive">Chart error:</span>{" "}
      <span className="text-muted-foreground">{message}</span>
    </div>
  );
}

export function InlineChart({ variant, content, onError }: RendererProps) {
  const chartType = variant || "bar";
  const [error, setError] = React.useState<string | null>(null);
  const [data, setData] = React.useState<ChartData | null>(null);
  const reportedRef = React.useRef<string | null>(null);

  // Report rendering errors upward so the AI gets feedback
  React.useEffect(() => {
    if (error && error !== reportedRef.current && onError) {
      reportedRef.current = error;
      onError(error);
    }
  }, [error, onError]);

  React.useEffect(() => {
    setError(null);
    setData(null);

    if (!VALID_TYPES.has(chartType)) {
      setError(`Unknown chart type "${chartType}". Valid: ${[...VALID_TYPES].join(", ")}`);
      return;
    }

    let parsed: ChartData;
    try {
      parsed = JSON.parse(content);
    } catch (e) {
      setError(`Invalid JSON: ${(e as Error).message}`);
      return;
    }

    if (!parsed.labels || !parsed.values) {
      setError('Missing required fields. Chart data must include "labels" and "values" arrays.');
      return;
    }

    if (!Array.isArray(parsed.labels) || !Array.isArray(parsed.values)) {
      setError('"labels" and "values" must be arrays.');
      return;
    }

    if (parsed.labels.length === 0 || parsed.values.length === 0) {
      setError("Labels and values arrays cannot be empty.");
      return;
    }

    if (parsed.labels.length !== parsed.values.length) {
      setError(
        `Labels (${parsed.labels.length}) and values (${parsed.values.length}) must have the same length.`,
      );
      return;
    }

    if (chartType === "line" && parsed.values.length < 2) {
      setError("Line chart requires at least 2 data points.");
      return;
    }

    setData(parsed);
  }, [chartType, content]);

  if (error) {
    return <ErrorBox message={error} />;
  }

  if (!data) return null;

  const palette = data.colors?.length ? data.colors : DEFAULT_COLORS;
  const width = data.width ?? 480;
  const height = data.height ?? 280;

  const chartData = data.labels!.map((label, i) => ({
    label,
    value: data.values![i],
    fill: palette[i % palette.length],
  }));

  const config: ChartConfig = Object.fromEntries(
    chartData.map((item) => [item.label, { label: item.label, color: item.fill }]),
  );

  const renderChart = () => {
    switch (chartType) {
      case "pie":
      case "donut":
        return (
          <PieChart>
            <Pie
              data={chartData}
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
              {chartData.map((entry, i) => (
                <Cell key={i} fill={entry.fill} />
              ))}
            </Pie>
            <ChartTooltip content={<InlineTooltip />} />
          </PieChart>
        );
      case "bar":
        return (
          <BarChart data={chartData}>
            <CartesianGrid vertical={false} />
            <XAxis dataKey="label" tickLine={false} axisLine={false} tickMargin={8} />
            <YAxis tickLine={false} axisLine={false} tickMargin={8} />
            <ChartTooltip content={<InlineTooltip />} />
            <Bar dataKey="value" radius={4}>
              {chartData.map((entry, i) => (
                <Cell key={i} fill={entry.fill} />
              ))}
            </Bar>
          </BarChart>
        );
      case "line":
        return (
          <LineChart data={chartData}>
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
  };

  return (
    <div className="my-2">
      {data.title && (
        <div className="mb-1 text-xs font-medium text-muted-foreground">{data.title}</div>
      )}
      <ChartContainer config={config} className="w-full" initialDimension={{ width, height }}>
        {renderChart()}
      </ChartContainer>
    </div>
  );
}
