import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { Panel } from "../ui/Panel";
import { Spinner } from "../ui/Spinner";
import { formatNum, formatPct } from "../../lib/format";
import type { SweepPoint } from "../../types/api";
import type { SweepStatus } from "../../hooks/useSweep";

/** Metrics where lower is better (invert when coloring). */
const LOWER_IS_BETTER = new Set(["max_drawdown"]);

/** Keys that should be formatted as percentages. */
const PCT_METRICS = new Set([
  "total_return", "cagr", "annualized_volatility", "max_drawdown", "win_rate",
]);

function getMetricValue(pt: SweepPoint, metric: string): number {
  return ((pt.metrics as unknown) as Record<string, number | null>)[metric] ?? -Infinity;
}

function formatMetric(value: number | null, metric: string): string {
  if (value === null || value === undefined || !isFinite(value as number)) return "—";
  return PCT_METRICS.has(metric) ? formatPct(value as number) : formatNum(value as number);
}

// ─── Color scale: 0 = worst (slate-700) → 1 = best (emerald-500) ─────────────

function metricColor(norm: number): string {
  // Interpolate from slate-700 (#334155) to emerald-500 (#10b981)
  const r = Math.round(0x33 + (0x10 - 0x33) * norm);
  const g = Math.round(0x41 + (0xb9 - 0x41) * norm);
  const b = Math.round(0x55 + (0x81 - 0x55) * norm);
  return `rgb(${r},${g},${b})`;
}

function normalize(
  values: number[],
  metric: string,
): (v: number) => number {
  const finite = values.filter(isFinite);
  if (finite.length === 0) return () => 0;
  const min = Math.min(...finite);
  const max = Math.max(...finite);
  const range = max - min;
  const lowerBetter = LOWER_IS_BETTER.has(metric);
  if (range === 0) return () => 0.5;
  return (v) => {
    const raw = (v - min) / range;
    return lowerBetter ? 1 - raw : raw;
  };
}

// ─── 1-param: bar chart ───────────────────────────────────────────────────────

function SweepBarChart({
  results,
  paramKey,
  metric,
}: {
  results: SweepPoint[];
  paramKey: string;
  metric: string;
}) {
  const sorted = [...results].sort((a, b) => a.params[paramKey] - b.params[paramKey]);
  const values = sorted.map((r) => getMetricValue(r, metric));
  const norm = normalize(values, metric);

  const data = sorted.map((r, i) => ({
    label: String(r.params[paramKey]),
    value: values[i],
    norm: norm(values[i]),
  }));

  return (
    <div className="h-72 w-full">
      <ResponsiveContainer>
        <BarChart data={data} margin={{ top: 8, right: 16, bottom: 8, left: 8 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis
            dataKey="label"
            tick={{ fontSize: 11, fill: "#94a3b8" }}
            label={{ value: paramKey, position: "insideBottom", offset: -4, fill: "#64748b", fontSize: 11 }}
          />
          <YAxis
            tick={{ fontSize: 11, fill: "#94a3b8" }}
            width={60}
            tickFormatter={(v) => formatMetric(v, metric)}
          />
          <Tooltip
            contentStyle={{ background: "#0f172a", border: "1px solid #334155", borderRadius: 8, fontSize: 12 }}
            formatter={(v: number) => [formatMetric(v, metric), metric]}
            labelFormatter={(l) => `${paramKey} = ${l}`}
          />
          <Bar dataKey="value" radius={[3, 3, 0, 0]}>
            {data.map((d, i) => (
              <Cell key={i} fill={metricColor(d.norm)} />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

// ─── 2-param: heatmap grid ───────────────────────────────────────────────────

function SweepHeatmap({
  results,
  param1,
  param2,
  metric,
}: {
  results: SweepPoint[];
  param1: string;
  param2: string;
  metric: string;
}) {
  const vals1 = [...new Set(results.map((r) => r.params[param1]))].sort((a, b) => a - b);
  const vals2 = [...new Set(results.map((r) => r.params[param2]))].sort((a, b) => a - b);

  const lookup = new Map(
    results.map((r) => [`${r.params[param1]},${r.params[param2]}`, getMetricValue(r, metric)]),
  );

  const allValues = results.map((r) => getMetricValue(r, metric));
  const norm = normalize(allValues, metric);

  const cellSize = Math.max(36, Math.min(64, Math.floor(560 / Math.max(vals1.length, vals2.length))));

  return (
    <div className="overflow-x-auto">
      <div className="inline-block min-w-full">
        {/* Column headers (param2 values) */}
        <div className="mb-1 flex" style={{ paddingLeft: cellSize + 8 }}>
          <p className="mb-1 text-xs text-slate-500" style={{ width: "100%" }}>
            {param2} →
          </p>
        </div>
        <div className="flex">
          {/* Row headers (param1 values) */}
          <div className="mr-2 flex flex-col items-end justify-center">
            <p
              className="text-xs text-slate-500"
              style={{
                writingMode: "vertical-rl",
                transform: "rotate(180deg)",
                height: cellSize * vals1.length,
              }}
            >
              {param1} ↓
            </p>
          </div>
          <div>
            {/* Column label row */}
            <div className="mb-1 flex gap-px">
              {vals2.map((v2) => (
                <div
                  key={v2}
                  className="text-center text-xs text-slate-400"
                  style={{ width: cellSize }}
                >
                  {v2}
                </div>
              ))}
            </div>
            {/* Grid */}
            {vals1.map((v1) => (
              <div key={v1} className="flex gap-px mb-px">
                {vals2.map((v2) => {
                  const val = lookup.get(`${v1},${v2}`) ?? NaN;
                  const n = isFinite(val) ? norm(val) : 0;
                  return (
                    <div
                      key={v2}
                      title={`${param1}=${v1}, ${param2}=${v2}: ${formatMetric(val, metric)}`}
                      className="flex items-center justify-center rounded-sm text-xs font-medium text-slate-900"
                      style={{
                        width: cellSize,
                        height: cellSize,
                        backgroundColor: isFinite(val) ? metricColor(n) : "#1e293b",
                        fontSize: cellSize > 48 ? 11 : 9,
                      }}
                    >
                      {isFinite(val) ? formatMetric(val, metric) : ""}
                    </div>
                  );
                })}
              </div>
            ))}
            {/* Row labels on the left side — inline with each row */}
          </div>
          {/* Row labels */}
          <div className="ml-2 flex flex-col gap-px">
            {vals1.map((v1) => (
              <div
                key={v1}
                className="flex items-center text-xs text-slate-400"
                style={{ height: cellSize }}
              >
                {v1}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Best combo summary ───────────────────────────────────────────────────────

function BestCombo({ results, metric }: { results: SweepPoint[]; metric: string }) {
  const finite = results.filter((r) => isFinite(getMetricValue(r, metric)));
  if (finite.length === 0) return null;
  const lowerBetter = LOWER_IS_BETTER.has(metric);
  const best = lowerBetter
    ? finite.reduce((a, b) => (getMetricValue(a, metric) < getMetricValue(b, metric) ? a : b))
    : finite.reduce((a, b) => (getMetricValue(a, metric) > getMetricValue(b, metric) ? a : b));

  return (
    <div className="rounded-md bg-emerald-950/40 border border-emerald-700/40 px-4 py-3 text-sm">
      <p className="mb-2 font-medium text-emerald-300">Best combination</p>
      <div className="flex flex-wrap gap-3">
        {Object.entries(best.params).map(([k, v]) => (
          <span key={k} className="text-slate-300">
            <span className="text-slate-500">{k}:</span>{" "}
            <span className="font-mono font-semibold">{v}</span>
          </span>
        ))}
        <span className="text-slate-300">
          <span className="text-slate-500">{metric}:</span>{" "}
          <span className="font-mono font-semibold text-emerald-400">
            {formatMetric(getMetricValue(best, metric), metric)}
          </span>
        </span>
      </div>
    </div>
  );
}

// ─── Main panel ──────────────────────────────────────────────────────────────

interface SweepResultsPanelProps {
  results: SweepPoint[];
  status: SweepStatus;
  error: string | null;
  metric: string;
}

export function SweepResultsPanel({
  results,
  status,
  error,
  metric,
}: SweepResultsPanelProps) {
  if (status === "idle") {
    return (
      <Panel>
        <p className="py-16 text-center text-slate-500">
          Configure a dataset, strategy, and parameter ranges, then run the sweep.
        </p>
      </Panel>
    );
  }

  if (status === "loading") {
    return (
      <Panel>
        <div className="flex justify-center py-16">
          <Spinner label="Running parameter sweep…" />
        </div>
      </Panel>
    );
  }

  if (status === "error") {
    return (
      <Panel title="Error">
        <p className="rounded-md bg-rose-950/60 px-3 py-2 text-sm text-rose-300">{error}</p>
      </Panel>
    );
  }

  if (results.length === 0) {
    return (
      <Panel title="Sweep results">
        <p className="py-8 text-center text-slate-500">No valid combinations found.</p>
      </Panel>
    );
  }

  // Determine which params actually vary
  const allParamKeys = Object.keys(results[0].params);
  const variedKeys = allParamKeys.filter((k) => {
    const vals = results.map((r) => r.params[k]);
    return new Set(vals).size > 1;
  });

  return (
    <div className="space-y-4">
      <Panel title={`Sweep results — ${results.length} combinations`}>
        <div className="mb-4">
          <BestCombo results={results} metric={metric} />
        </div>
        {variedKeys.length === 1 ? (
          <SweepBarChart results={results} paramKey={variedKeys[0]} metric={metric} />
        ) : variedKeys.length === 2 ? (
          <SweepHeatmap
            results={results}
            param1={variedKeys[0]}
            param2={variedKeys[1]}
            metric={metric}
          />
        ) : (
          <p className="text-sm text-slate-400">
            {variedKeys.length === 0
              ? "All parameters were fixed — run with at least one varying parameter."
              : "More than 2 parameters vary — reduce to 1 or 2 for a chart."}
          </p>
        )}
      </Panel>
    </div>
  );
}
