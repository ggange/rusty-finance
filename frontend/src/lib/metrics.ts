// Metric naming and formatting shared by the sweep and walk-forward panels,
// which both read arbitrary metric keys out of a Metrics object by name.

import { formatNum, formatPct } from "./format";
import type { Interval, Metrics } from "../types/api";

/** Metrics where a lower value is better, so colour scales must invert. */
export const LOWER_IS_BETTER = new Set(["max_drawdown", "annualized_volatility"]);

/** Metrics stored as fractions and displayed as percentages. */
export const PCT_METRICS = new Set([
  "total_return",
  "cagr",
  "annualized_volatility",
  "max_drawdown",
  "win_rate",
]);

/** Metric options offered by the sweep and walk-forward forms. */
export const METRIC_OPTIONS = [
  { value: "sharpe_ratio", label: "Sharpe ratio" },
  { value: "total_return", label: "Total return" },
  { value: "cagr", label: "CAGR" },
  { value: "sortino_ratio", label: "Sortino ratio" },
  { value: "max_drawdown", label: "Max drawdown (lower is better)" },
  { value: "annualized_volatility", label: "Ann. volatility (lower is better)" },
];

/**
 * Read a metric by name. Metrics is a fixed struct on the Rust side but the UI
 * selects a key at runtime, so the index has to be widened.
 */
export function getMetricValue(metrics: Metrics, key: string, fallback = 0): number {
  const v = (metrics as unknown as Record<string, number | null>)[key];
  return v ?? fallback;
}

export function formatMetric(value: number | null, metric: string): string {
  if (value === null || value === undefined || !isFinite(value)) return "—";
  return PCT_METRICS.has(metric) ? formatPct(value) : formatNum(value);
}

/** Metrics the bootstrap reports a two-sided interval for. */
const INTERVAL_METRICS = new Set(["sharpe_ratio", "sortino_ratio", "cagr"]);

/**
 * Read a metric's confidence interval, or null when there isn't one.
 *
 * Deliberately separate from `getMetricValue` rather than folding the interval
 * into the metric itself: that function widens `Metrics` to
 * `Record<string, number | null>` so the sweep and walk-forward panels can index
 * by a runtime key, and those numbers feed chart scales directly. An object
 * where a number was expected would break the charts silently.
 */
export function getMetricInterval(metrics: Metrics, key: string): Interval | null {
  const u = metrics.uncertainty;
  if (!u || !INTERVAL_METRICS.has(key)) return null;
  return (u as unknown as Record<string, Interval>)[key] ?? null;
}

/**
 * "95% CI −2.41 … 4.83", or undefined when there is no interval — so it drops
 * straight into an optional prop rather than rendering an em dash.
 *
 * The confidence label comes from the response, not a hardcoded 95, so a tuned
 * request renders honestly.
 */
export function formatInterval(
  iv: Interval | null,
  metric: string,
  confidence?: number,
): string | undefined {
  if (!iv) return undefined;
  const pct = Math.round((confidence ?? 0.95) * 100);
  return `${pct}% CI ${formatMetric(iv.lo, metric)} … ${formatMetric(iv.hi, metric)}`;
}

/**
 * Max drawdown gets a spread but no endpoints, so it renders as "± 5.4pp"
 * instead of an interval. See `MetricUncertainty.max_drawdown_std_error`.
 */
export function formatDrawdownSpread(metrics: Metrics): string | undefined {
  const se = metrics.uncertainty?.max_drawdown_std_error;
  if (se === undefined || se === null || !isFinite(se)) return undefined;
  return `± ${(se * 100).toFixed(1)}pp (1 s.e.)`;
}
