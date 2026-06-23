import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { formatCurrency } from "../../lib/format";
import type { Candle, EquityPoint } from "../../types/api";

export interface BenchmarkAsset {
  candles: Candle[];
  allocatedCash: number;
}

interface PortfolioEquityChartProps {
  equityCurve: EquityPoint[];
  assets: BenchmarkAsset[];
  externalBenchmarkCurve?: EquityPoint[];
  rebalanceDates?: string[];
}

/**
 * Portfolio NAV vs a weighted buy-and-hold benchmark. The benchmark is the sum,
 * across assets, of each asset's buy-and-hold mark-to-market (buy whole shares
 * with the asset's allocated cash on its first bar). Aligned to portfolio dates
 * by carrying each asset's last known close forward.
 */
export function PortfolioEquityChart({
  equityCurve,
  assets,
  externalBenchmarkCurve,
  rebalanceDates,
}: PortfolioEquityChartProps) {
  // Precompute per-asset buy-and-hold parameters and a date→close lookup.
  const holdings = assets.map((a) => {
    const firstClose = a.candles[0]?.close ?? 0;
    const shares = firstClose > 0 ? Math.floor(a.allocatedCash / firstClose) : 0;
    const cashRem = a.allocatedCash - shares * firstClose;
    const byDate = new Map(a.candles.map((c) => [c.date, c.close]));
    return { shares, cashRem, byDate, lastClose: firstClose };
  });

  // Build a date→nav lookup for the external benchmark, carrying forward the last known value.
  const extByDate = externalBenchmarkCurve
    ? new Map(externalBenchmarkCurve.map((p) => [p.date, p.nav]))
    : null;
  let lastExtNav: number | undefined;

  const data = equityCurve.map((pt) => {
    let benchmark = 0;
    for (const h of holdings) {
      const close = h.byDate.get(pt.date);
      if (close !== undefined) h.lastClose = close;
      benchmark += h.shares * h.lastClose + h.cashRem;
    }
    let extBenchmark: number | undefined;
    if (extByDate) {
      const v = extByDate.get(pt.date);
      if (v !== undefined) lastExtNav = v;
      extBenchmark = lastExtNav;
    }
    return { date: pt.date, strategy: pt.nav, benchmark, extBenchmark };
  });

  return (
    <div className="h-72 w-full">
      <ResponsiveContainer>
        <LineChart data={data} margin={{ top: 8, right: 16, bottom: 8, left: 8 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="date" tick={{ fontSize: 11, fill: "#94a3b8" }} minTickGap={40} />
          <YAxis
            tick={{ fontSize: 11, fill: "#94a3b8" }}
            width={70}
            tickFormatter={(v) => formatCurrency(v, 0)}
            domain={["auto", "auto"]}
          />
          <Tooltip
            contentStyle={{
              background: "#0f172a",
              border: "1px solid #334155",
              borderRadius: 8,
              fontSize: 12,
            }}
            formatter={(v: number) => formatCurrency(v)}
          />
          <Legend wrapperStyle={{ fontSize: 12 }} />
          <Line
            type="monotone"
            dataKey="strategy"
            name="Portfolio"
            stroke="#38bdf8"
            strokeWidth={2}
            dot={false}
          />
          <Line
            type="monotone"
            dataKey="benchmark"
            name="Buy & Hold"
            stroke="#a3a3a3"
            strokeWidth={1.5}
            strokeDasharray="4 4"
            dot={false}
          />
          {extByDate && (
            <Line
              type="monotone"
              dataKey="extBenchmark"
              name="Benchmark"
              stroke="#fb923c"
              strokeWidth={1.5}
              strokeDasharray="6 2"
              dot={false}
            />
          )}
          {rebalanceDates?.map((d) => (
            <ReferenceLine
              key={d}
              x={d}
              stroke="#7c3aed"
              strokeWidth={1}
              strokeDasharray="2 4"
              label={{ value: "↺", position: "insideTopRight", fill: "#7c3aed", fontSize: 10 }}
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
