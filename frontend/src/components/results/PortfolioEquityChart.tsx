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
import {
  AXIS_TICK,
  CHART_MARGIN,
  GRID,
  GRID_DASH,
  LEGEND_STYLE,
  SERIES,
  TOOLTIP_STYLE,
} from "../../lib/chartTheme";
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
        <LineChart data={data} margin={CHART_MARGIN}>
          <CartesianGrid strokeDasharray={GRID_DASH} stroke={GRID} />
          <XAxis dataKey="date" tick={AXIS_TICK} minTickGap={40} />
          <YAxis
            tick={AXIS_TICK}
            width={70}
            tickFormatter={(v) => formatCurrency(v, 0)}
            domain={["auto", "auto"]}
          />
          <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v: number) => formatCurrency(v)} />
          <Legend wrapperStyle={LEGEND_STYLE} />
          <Line
            type="monotone"
            dataKey="strategy"
            name="Portfolio"
            stroke={SERIES.primary}
            strokeWidth={2}
            dot={false}
          />
          <Line
            type="monotone"
            dataKey="benchmark"
            name="Buy & Hold"
            stroke={SERIES.benchmark}
            strokeWidth={1.5}
            strokeDasharray="4 4"
            dot={false}
          />
          {extByDate && (
            <Line
              type="monotone"
              dataKey="extBenchmark"
              name="Benchmark"
              stroke={SERIES.external}
              strokeWidth={1.5}
              strokeDasharray="6 2"
              dot={false}
            />
          )}
          {rebalanceDates?.map((d) => (
            <ReferenceLine
              key={d}
              x={d}
              stroke={SERIES.marker}
              strokeWidth={1}
              strokeDasharray="2 4"
              label={{
                value: "↺",
                position: "insideTopRight",
                fill: SERIES.marker,
                fontSize: 10,
              }}
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
