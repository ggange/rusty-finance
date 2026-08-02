// Metric naming and formatting shared by the sweep and walk-forward panels,
// which both read arbitrary metric keys out of a Metrics object by name.

import { formatNum, formatPct } from "./format";
import type { Metrics } from "../types/api";

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
