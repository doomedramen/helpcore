"use client";

import * as React from "react";
import {
  Area,
  AreaChart,
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
import {
  ChartContainer,
  ChartTooltip,
  ChartLegend,
  ChartLegendContent,
  type ChartConfig,
} from "@/app/components/ui/chart";
import type { RendererProps } from "./content-renderers";

const CHART_CSS_VARS = [
  "var(--chart-1)",
  "var(--chart-2)",
  "var(--chart-3)",
  "var(--chart-4)",
  "var(--chart-5)",
];

const EXTRA_COLORS = ["#8B5CF6", "#EC4899", "#F97316", "#14B8A6", "#6366F1"];

const LIGHT_PALETTE = ["#4F8FF9", "#F95D6A", "#2EC4B6", "#FFB85C", "#A78BFA"];
const DARK_PALETTE = ["#6DB3FF", "#FF7B85", "#4DD9CC", "#FFC87A", "#BBA4FC"];

const VALID_TYPES = new Set(["pie", "donut", "bar", "line", "area"]);

interface Dataset {
  label?: string;
  values: number[];
  color?: string;
  fill?: string;
}

interface ChartData {
  title?: string;
  labels?: string[];
  values?: number[];
  datasets?: Dataset[];
  colors?: string[];
  width?: number;
  height?: number;
  horizontal?: boolean;
  stacked?: boolean;
  theme?: "auto" | "dark" | "light";
  responsive?: boolean;
}

function resolvePalette(
  theme: "auto" | "dark" | "light" | undefined,
  customColors?: string[],
): string[] {
  if (customColors?.length) return customColors;
  if (theme === "dark") return DARK_PALETTE;
  if (theme === "light") return LIGHT_PALETTE;
  return []; // auto: CSS variables handle it
}

function pickColor(index: number, palette: string[], useCssVars: boolean): string {
  if (useCssVars && index < CHART_CSS_VARS.length) {
    return CHART_CSS_VARS[index];
  }
  const colors = [...CHART_CSS_VARS, ...EXTRA_COLORS];
  if (useCssVars && index < colors.length) {
    return colors[index];
  }
  const fallback = [...palette, ...EXTRA_COLORS];
  return fallback[index % fallback.length];
}

function InlineTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: { name?: string; payload: Record<string, unknown>; value: number }[];
}) {
  if (!active || !payload?.length) return null;
  return (
    <div className="rounded-lg border border-border/50 bg-background px-2.5 py-1.5 text-xs shadow-xl">
      {payload.map((item, i) => (
        <div key={i} className="flex items-center gap-2 whitespace-nowrap">
          <span
            className="inline-block h-2 w-2 shrink-0 rounded-[2px]"
            style={{ backgroundColor: item.payload.fill as string }}
          />
          <span className="text-muted-foreground">
            {item.name && item.name !== "value" ? item.name : String(item.payload.label ?? "")}
          </span>
          <span className="font-mono font-medium tabular-nums">{item.value.toLocaleString()}</span>
        </div>
      ))}
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

function EmptyState({ title }: { title?: string }) {
  return (
    <div className="rounded-lg border border-border/50 bg-muted/20 px-4 py-6 text-center">
      <div className="text-xs font-medium text-muted-foreground">
        {title ? `${title}: ` : ""}No data available
      </div>
    </div>
  );
}

export function InlineChart({ variant, content, onError }: RendererProps) {
  const chartType = variant || "bar";
  const [error, setError] = React.useState<string | null>(null);
  const [data, setData] = React.useState<ChartData | null>(null);
  const [isEmpty, setIsEmpty] = React.useState(false);
  const reportedRef = React.useRef<string | null>(null);
  const containerRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    if (error && error !== reportedRef.current && onError) {
      reportedRef.current = error;
      onError(error);
    }
  }, [error, onError]);

  const handleDownload = React.useCallback(() => {
    const container = containerRef.current;
    if (!container) return;
    const svg = container.querySelector("svg");
    if (!svg) return;

    const svgData = new XMLSerializer().serializeToString(svg);
    const blob = new Blob([svgData], { type: "image/svg+xml;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `chart-${Date.now()}.svg`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }, []);

  React.useEffect(() => {
    setError(null);
    setData(null);
    setIsEmpty(false);

    if (!VALID_TYPES.has(chartType)) {
      setError(`Unknown chart type "${chartType}". Valid: ${[...VALID_TYPES].sort().join(", ")}`);
      return;
    }

    let parsed: ChartData;
    try {
      parsed = JSON.parse(content);
    } catch (e) {
      setError(`Invalid JSON: ${(e as Error).message}`);
      return;
    }

    const hasDatasets = parsed.datasets && parsed.datasets.length > 0;

    if (!parsed.labels && !hasDatasets) {
      setError(
        'Missing required fields. Chart data must include "labels" and either "values" or "datasets".',
      );
      return;
    }

    if (!hasDatasets && (!parsed.values || !Array.isArray(parsed.values))) {
      setError(
        'Missing required fields. Chart data must include "values" (number array) or "datasets".',
      );
      return;
    }

    if (hasDatasets) {
      for (const ds of parsed.datasets!) {
        if (!ds.values || !Array.isArray(ds.values)) {
          setError('Each dataset must have a "values" array.');
          return;
        }
      }
    }

    const labels = parsed.labels ?? [];
    const allValues = hasDatasets
      ? parsed.datasets!.flatMap((d) => d.values)
      : (parsed.values ?? []);

    if (allValues.every((v) => v === 0 || v == null)) {
      setData(parsed);
      setIsEmpty(true);
      return;
    }

    if (hasDatasets) {
      for (let i = 0; i < parsed.datasets!.length; i++) {
        const ds = parsed.datasets![i];
        if (labels.length > 0 && ds.values.length !== labels.length) {
          setError(
            `Dataset ${i} values (${ds.values.length}) must match labels length (${labels.length}).`,
          );
          return;
        }
      }
    } else if (parsed.values!.length !== labels.length) {
      setError(
        `Labels (${labels.length}) and values (${parsed.values!.length}) must have the same length.`,
      );
      return;
    }

    if (chartType === "line" && allValues.length < 2) {
      setError("Line chart requires at least 2 data points.");
      return;
    }

    setData(parsed);
  }, [chartType, content]);

  if (error) {
    return <ErrorBox message={error} />;
  }

  if (!data) return null;

  if (isEmpty) {
    return <EmptyState title={data.title} />;
  }

  const palette = resolvePalette(data.theme, data.colors);
  const useCssVars = !data.colors?.length && (!data.theme || data.theme === "auto");
  const width = data.width ?? 480;
  const height = data.height ?? 280;
  const isHorizontal = data.horizontal === true;
  const isStacked = data.stacked === true;
  const isResponsive = data.responsive === true;

  const hasDatasets = !!data.datasets?.length;
  const labels = data.labels ?? [];
  const datasetEntries = hasDatasets
    ? data.datasets!
    : [{ label: data.title, values: data.values!, color: palette[0] }];

  const datasetKeys = hasDatasets
    ? datasetEntries.map((ds) => ds.label ?? `Series ${datasetEntries.indexOf(ds) + 1}`)
    : ["value"];

  const chartData =
    labels.length > 0
      ? labels.map((label, i) => {
          const row: Record<string, unknown> = { label };
          for (let j = 0; j < datasetEntries.length; j++) {
            const key = datasetKeys[j];
            row[key] = datasetEntries[j].values[i];
          }
          return row;
        })
      : datasetEntries[0].values.map((_, i) => {
          const row: Record<string, unknown> = { label: `#${i + 1}` };
          for (let j = 0; j < datasetEntries.length; j++) {
            const key = datasetKeys[j];
            row[key] = datasetEntries[j].values[i];
          }
          return row;
        });

  const config: ChartConfig = {};
  for (let j = 0; j < datasetEntries.length; j++) {
    const ds = datasetEntries[j];
    const key = datasetKeys[j];
    const color = ds.color ?? ds.fill ?? pickColor(j, palette, useCssVars);
    config[key] = { label: ds.label ?? key, color };
  }

  const renderChart = () => {
    switch (chartType) {
      case "pie":
      case "donut": {
        const pieData = datasetEntries[0].values.map((value, i) => ({
          label: labels[i] ?? `#${i + 1}`,
          value,
          fill: data.colors?.[i % data.colors.length] ?? palette[i % palette.length],
        }));

        return (
          <PieChart>
            <Pie
              data={pieData}
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
              isAnimationActive
              animationDuration={400}
            >
              {pieData.map((entry, i) => (
                <Cell key={i} fill={entry.fill} />
              ))}
            </Pie>
            <ChartTooltip content={<InlineTooltip />} />
          </PieChart>
        );
      }

      case "bar": {
        const stackId = isStacked ? "stack" : undefined;
        return (
          <BarChart data={chartData} layout={isHorizontal ? "vertical" : "horizontal"}>
            <CartesianGrid vertical={false} horizontal={!isHorizontal} />
            {isHorizontal ? (
              <>
                <XAxis type="number" tickLine={false} axisLine={false} tickMargin={8} />
                <YAxis
                  dataKey="label"
                  type="category"
                  tickLine={false}
                  axisLine={false}
                  tickMargin={8}
                  width={120}
                  tick={{ fontSize: 11 }}
                />
              </>
            ) : (
              <>
                <XAxis
                  dataKey="label"
                  tickLine={false}
                  axisLine={false}
                  tickMargin={8}
                  angle={labels.some((l) => l.length > 8) ? -35 : undefined}
                  textAnchor={labels.some((l) => l.length > 8) ? "end" : undefined}
                  height={labels.some((l) => l.length > 8) ? 60 : undefined}
                  tick={{ fontSize: 11 }}
                />
                <YAxis tickLine={false} axisLine={false} tickMargin={8} />
              </>
            )}
            <ChartTooltip content={<InlineTooltip />} />
            {datasetEntries.length > 1 && <ChartLegend content={<ChartLegendContent />} />}
            {datasetEntries.map((ds, j) => {
              const key = datasetKeys[j];
              const color = ds.color ?? ds.fill ?? pickColor(j, palette, useCssVars);
              return (
                <Bar
                  key={key}
                  dataKey={key}
                  fill={color}
                  radius={4}
                  stackId={stackId}
                  isAnimationActive
                  animationDuration={400}
                />
              );
            })}
          </BarChart>
        );
      }

      case "line": {
        return (
          <LineChart data={chartData}>
            <CartesianGrid vertical={false} />
            <XAxis
              dataKey="label"
              tickLine={false}
              axisLine={false}
              tickMargin={8}
              angle={labels.some((l) => l.length > 8) ? -35 : undefined}
              textAnchor={labels.some((l) => l.length > 8) ? "end" : undefined}
              height={labels.some((l) => l.length > 8) ? 60 : undefined}
              tick={{ fontSize: 11 }}
            />
            <YAxis tickLine={false} axisLine={false} tickMargin={8} />
            <ChartTooltip content={<InlineTooltip />} />
            {datasetEntries.length > 1 && <ChartLegend content={<ChartLegendContent />} />}
            {datasetEntries.map((ds, j) => {
              const key = datasetKeys[j];
              const color = ds.color ?? ds.fill ?? pickColor(j, palette, useCssVars);
              return (
                <Line
                  key={key}
                  dataKey={key}
                  name={ds.label ?? key}
                  stroke={color}
                  strokeWidth={2}
                  dot={{ fill: color }}
                  activeDot={{ r: 4 }}
                  isAnimationActive
                  animationDuration={400}
                />
              );
            })}
          </LineChart>
        );
      }

      case "area": {
        const stackId = isStacked ? "stack" : undefined;
        return (
          <AreaChart data={chartData}>
            <CartesianGrid vertical={false} />
            <XAxis
              dataKey="label"
              tickLine={false}
              axisLine={false}
              tickMargin={8}
              angle={labels.some((l) => l.length > 8) ? -35 : undefined}
              textAnchor={labels.some((l) => l.length > 8) ? "end" : undefined}
              height={labels.some((l) => l.length > 8) ? 60 : undefined}
              tick={{ fontSize: 11 }}
            />
            <YAxis tickLine={false} axisLine={false} tickMargin={8} />
            <ChartTooltip content={<InlineTooltip />} />
            {datasetEntries.length > 1 && <ChartLegend content={<ChartLegendContent />} />}
            {datasetEntries.map((ds, j) => {
              const key = datasetKeys[j];
              const color = ds.color ?? ds.fill ?? pickColor(j, palette, useCssVars);
              return (
                <Area
                  key={key}
                  dataKey={key}
                  name={ds.label ?? key}
                  fill={color}
                  stroke={color}
                  fillOpacity={0.15}
                  strokeWidth={2}
                  type="monotone"
                  stackId={stackId}
                  isAnimationActive
                  animationDuration={400}
                />
              );
            })}
          </AreaChart>
        );
      }

      default:
        return null;
    }
  };

  return (
    <div className="my-2 group relative" ref={containerRef}>
      {data.title && (
        <div className="mb-1 flex items-center justify-between">
          <div className="text-xs font-medium text-muted-foreground">{data.title}</div>
          <button
            type="button"
            onClick={handleDownload}
            className="cursor-pointer rounded px-1.5 py-0.5 text-xs text-muted-foreground/60 opacity-0 transition hover:text-muted-foreground group-hover:opacity-100"
            title="Download chart as SVG"
          >
            Download
          </button>
        </div>
      )}
      {!data.title && (
        <button
          type="button"
          onClick={handleDownload}
          className="absolute top-1 right-1 z-10 cursor-pointer rounded bg-background/80 px-1.5 py-0.5 text-xs text-muted-foreground/60 opacity-0 transition hover:text-muted-foreground group-hover:opacity-100"
          title="Download chart as SVG"
        >
          Download
        </button>
      )}
      <ChartContainer
        config={config}
        className={isResponsive ? "w-full" : ""}
        initialDimension={{ width, height }}
      >
        {renderChart()}
      </ChartContainer>
    </div>
  );
}
