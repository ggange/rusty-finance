import { useState } from "react";
import { Panel } from "../ui/Panel";
import { Spinner } from "../ui/Spinner";
import { MetricCards } from "./MetricCards";
import { PortfolioEquityChart } from "./PortfolioEquityChart";
import { DrawdownChart } from "./DrawdownChart";
import { AssetBreakdown } from "./AssetBreakdown";
import { RiskPanel } from "./RiskPanel";
import type { PortfolioStatus } from "../../hooks/usePortfolio";
import type { Candle, PortfolioResponse } from "../../types/api";

/** Candles for each asset that was run, in request order. */
export interface RanAsset {
  symbol: string;
  candles: Candle[];
}

interface PortfolioResultsPanelProps {
  status: PortfolioStatus;
  error: string | null;
  result: PortfolioResponse | null;
  ranAssets: RanAsset[];
}

export function PortfolioResultsPanel({
  status,
  error,
  result,
  ranAssets,
}: PortfolioResultsPanelProps) {
  const [expanded, setExpanded] = useState<string | null>(null);

  if (status === "idle") {
    return (
      <Panel>
        <p className="py-16 text-center text-slate-500">
          Configure one or more assets, pick their data and strategies, then run
          a backtest to see portfolio results.
        </p>
      </Panel>
    );
  }

  if (status === "loading") {
    return (
      <Panel>
        <div className="flex justify-center py-16">
          <Spinner label="Running portfolio backtest…" />
        </div>
      </Panel>
    );
  }

  if (status === "error") {
    return (
      <Panel title="Error">
        <p className="rounded-md bg-rose-950/60 px-3 py-2 text-sm text-rose-300">
          {error}
        </p>
      </Panel>
    );
  }

  if (!result) return null;

  // Match each asset result to the candles it ran with (request order preserved;
  // fall back to a symbol lookup for safety).
  const candlesFor = (symbol: string, i: number): Candle[] =>
    ranAssets[i]?.symbol === symbol
      ? ranAssets[i].candles
      : (ranAssets.find((r) => r.symbol === symbol)?.candles ?? []);

  const benchmarkAssets = result.assets.map((a, i) => ({
    candles: candlesFor(a.symbol, i),
    allocatedCash: a.allocated_cash,
  }));

  return (
    <div className="space-y-4">
      <Panel title="Portfolio metrics">
        <MetricCards metrics={result.metrics} benchmark={result.benchmark} />
      </Panel>

      <Panel title="Portfolio equity vs. buy & hold">
        <PortfolioEquityChart
          equityCurve={result.equity_curve}
          assets={benchmarkAssets}
          externalBenchmarkCurve={result.external_benchmark_curve}
        />
      </Panel>

      <Panel title="Portfolio drawdown">
        <DrawdownChart equityCurve={result.equity_curve} />
      </Panel>

      <Panel title="Risk analytics">
        <RiskPanel risk={result.risk} />
      </Panel>

      <Panel title="By asset">
        <div className="space-y-2">
          {result.assets.map((a, i) => (
            <AssetBreakdown
              key={`${a.symbol}-${i}`}
              asset={a}
              candles={candlesFor(a.symbol, i)}
              expanded={expanded === `${a.symbol}-${i}`}
              onToggle={() =>
                setExpanded((cur) =>
                  cur === `${a.symbol}-${i}` ? null : `${a.symbol}-${i}`,
                )
              }
            />
          ))}
        </div>
      </Panel>
    </div>
  );
}
