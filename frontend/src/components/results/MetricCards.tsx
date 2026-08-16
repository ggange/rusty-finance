import { MetricCard } from "./MetricCard";
import { formatInt, formatNum, formatPct, signClass } from "../../lib/format";
import {
  formatDrawdownSpread,
  formatInterval,
  getMetricInterval,
} from "../../lib/metrics";
import type { Benchmark, Metrics } from "../../types/api";

interface MetricCardsProps {
  metrics: Metrics;
  benchmark: Benchmark;
}

export function MetricCards({ metrics, benchmark }: MetricCardsProps) {
  const returnDelta = metrics.total_return - benchmark.total_return;
  const cagrDelta = metrics.cagr - benchmark.cagr;

  const u = metrics.uncertainty;
  const ci = (key: string) =>
    formatInterval(getMetricInterval(metrics, key), key, u?.confidence);
  // Stated on hover rather than in the card, which has no room for it — and it
  // is the caveat that stops an interval being read as "now properly bounded".
  const ciTitle = u
    ? `${u.method} over ${u.observations} returns, ${u.resamples} resamples, ` +
      `mean block ${u.mean_block.toFixed(1)} bars. Sampling error only — this does ` +
      `not correct for parameter selection or for repeated trials across assets.`
    : undefined;

  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
      <MetricCard
        label="Total Return"
        value={formatPct(metrics.total_return)}
        valueClass={signClass(metrics.total_return)}
        benchmark={`bench ${formatPct(benchmark.total_return)} · Δ ${formatPct(returnDelta)}`}
      />
      <MetricCard
        label="CAGR"
        value={formatPct(metrics.cagr)}
        valueClass={signClass(metrics.cagr)}
        interval={ci("cagr")}
        intervalTitle={ciTitle}
        benchmark={`bench ${formatPct(benchmark.cagr)} · Δ ${formatPct(cagrDelta)}`}
      />
      <MetricCard
        label="Max Drawdown"
        value={formatPct(metrics.max_drawdown)}
        valueClass={signClass(metrics.max_drawdown)}
        interval={formatDrawdownSpread(metrics)}
        intervalTitle="Spread only. Block resampling breaks up the multi-month trends that produce deep drawdowns, so resampled drawdowns run milder than the observed one and percentile endpoints would read as optimistic."
      />
      <MetricCard
        label="Sharpe"
        value={formatNum(metrics.sharpe_ratio)}
        valueClass={signClass(metrics.sharpe_ratio)}
        interval={ci("sharpe_ratio")}
        intervalTitle={ciTitle}
      />
      <MetricCard
        label="Sortino"
        value={formatNum(metrics.sortino_ratio)}
        valueClass={signClass(metrics.sortino_ratio)}
        interval={ci("sortino_ratio")}
        intervalTitle={ciTitle}
      />
      <MetricCard
        label="Ann. Volatility"
        value={formatPct(metrics.annualized_volatility)}
      />
      <MetricCard
        label="Win Rate"
        value={formatPct(metrics.win_rate)}
      />
      <MetricCard label="Trades" value={formatInt(metrics.trade_count)} />
    </div>
  );
}
