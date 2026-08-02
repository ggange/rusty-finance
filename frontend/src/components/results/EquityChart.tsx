import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
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

interface EquityChartProps {
  equityCurve: EquityPoint[];
  candles: Candle[];
  initialCash: number;
}

/**
 * Strategy NAV vs a buy-and-hold benchmark. The benchmark series is computed
 * from candle closes exactly as the backend's Benchmark does: buy as many whole
 * shares as initial_cash allows on the first bar, then mark to market each day.
 */
export function EquityChart({
  equityCurve,
  candles,
  initialCash,
}: EquityChartProps) {
  const firstClose = candles[0]?.close ?? 0;
  const shares = firstClose > 0 ? Math.floor(initialCash / firstClose) : 0;
  const cashRem = initialCash - shares * firstClose;

  const data = equityCurve.map((pt, i) => ({
    date: pt.date,
    strategy: pt.nav,
    benchmark: shares * (candles[i]?.close ?? firstClose) + cashRem,
  }));

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
            name="Strategy"
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
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
