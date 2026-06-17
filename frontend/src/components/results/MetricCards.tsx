import { MetricCard } from "./MetricCard";
import { formatInt, formatNum, formatPct, signClass } from "../../lib/format";
import type { Benchmark, Metrics } from "../../types/api";

interface MetricCardsProps {
  metrics: Metrics;
  benchmark: Benchmark;
}

export function MetricCards({ metrics, benchmark }: MetricCardsProps) {
  const returnDelta = metrics.total_return - benchmark.total_return;
  const cagrDelta = metrics.cagr - benchmark.cagr;

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
        benchmark={`bench ${formatPct(benchmark.cagr)} · Δ ${formatPct(cagrDelta)}`}
      />
      <MetricCard
        label="Max Drawdown"
        value={formatPct(metrics.max_drawdown)}
        valueClass={signClass(metrics.max_drawdown)}
      />
      <MetricCard
        label="Sharpe"
        value={formatNum(metrics.sharpe_ratio)}
        valueClass={signClass(metrics.sharpe_ratio)}
      />
      <MetricCard
        label="Sortino"
        value={formatNum(metrics.sortino_ratio)}
        valueClass={signClass(metrics.sortino_ratio)}
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
