import { Panel } from "../ui/Panel";
import { Spinner } from "../ui/Spinner";
import { MetricCards } from "./MetricCards";
import { EquityChart } from "./EquityChart";
import { DrawdownChart } from "./DrawdownChart";
import { PriceChart } from "./PriceChart";
import { TradeLogTable } from "./TradeLogTable";
import type { BacktestStatus } from "../../hooks/useBacktest";
import type { BacktestResponse, Candle } from "../../types/api";

interface ResultsPanelProps {
  status: BacktestStatus;
  error: string | null;
  result: BacktestResponse | null;
  candles: Candle[];
  initialCash: number;
}

export function ResultsPanel({
  status,
  error,
  result,
  candles,
  initialCash,
}: ResultsPanelProps) {
  if (status === "idle") {
    return (
      <Panel>
        <p className="py-16 text-center text-slate-500">
          Load price data, pick a strategy, and run a backtest to see results.
        </p>
      </Panel>
    );
  }

  if (status === "loading") {
    return (
      <Panel>
        <div className="flex justify-center py-16">
          <Spinner label="Running backtest…" />
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

  return (
    <div className="space-y-4">
      <Panel title="Metrics">
        <MetricCards metrics={result.metrics} benchmark={result.benchmark} />
      </Panel>

      <Panel title="Equity curve vs. buy & hold">
        <EquityChart
          equityCurve={result.equity_curve}
          candles={candles}
          initialCash={initialCash}
        />
      </Panel>

      <Panel title="Drawdown">
        <DrawdownChart equityCurve={result.equity_curve} />
      </Panel>

      <Panel title="Price &amp; trades">
        <PriceChart candles={candles} trades={result.trades} />
      </Panel>

      <Panel title="Trade log">
        <TradeLogTable trades={result.trades} />
      </Panel>
    </div>
  );
}
