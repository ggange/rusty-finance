import { MetricCards } from "./MetricCards";
import { EquityChart } from "./EquityChart";
import { DrawdownChart } from "./DrawdownChart";
import { PriceChart } from "./PriceChart";
import { TradeLogTable } from "./TradeLogTable";
import { formatCurrency, formatPct, signClass } from "../../lib/format";
import type { AssetResult, Candle } from "../../types/api";

interface AssetBreakdownProps {
  asset: AssetResult;
  candles: Candle[];
  expanded: boolean;
  onToggle: () => void;
}

export function AssetBreakdown({
  asset,
  candles,
  expanded,
  onToggle,
}: AssetBreakdownProps) {
  return (
    <div className="rounded-lg border border-slate-700 bg-slate-900/40">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center justify-between px-4 py-3 text-left"
        aria-expanded={expanded}
      >
        <div className="flex items-center gap-3">
          <span className="text-slate-400">{expanded ? "▾" : "▸"}</span>
          <span className="font-semibold text-slate-100">{asset.symbol}</span>
          <span className="text-xs text-slate-500">
            {(asset.weight * 100).toFixed(0)}% · {formatCurrency(asset.allocated_cash, 0)}
          </span>
        </div>
        <div className="flex items-center gap-4 text-sm tabular-nums">
          <span className={signClass(asset.metrics.total_return)}>
            {formatPct(asset.metrics.total_return)}
          </span>
          <span className="text-xs text-slate-500">
            {asset.trades.length} trade{asset.trades.length === 1 ? "" : "s"}
          </span>
        </div>
      </button>

      {expanded && (
        <div className="space-y-4 border-t border-slate-700 p-4">
          <MetricCards metrics={asset.metrics} benchmark={asset.benchmark} />
          <div>
            <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
              Equity vs. buy &amp; hold
            </p>
            <EquityChart
              equityCurve={asset.equity_curve}
              candles={candles}
              initialCash={asset.allocated_cash}
            />
          </div>
          <div>
            <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
              Drawdown
            </p>
            <DrawdownChart equityCurve={asset.equity_curve} />
          </div>
          {candles.length > 0 && (
            <div>
              <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
                Price &amp; trades
              </p>
              <PriceChart candles={candles} trades={asset.trades} />
            </div>
          )}
          <div>
            <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
              Trade log
            </p>
            <TradeLogTable trades={asset.trades} />
          </div>
        </div>
      )}
    </div>
  );
}
