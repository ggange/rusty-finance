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
            name="Strategy"
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
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
