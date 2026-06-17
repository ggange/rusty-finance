import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { deriveDrawdown } from "../../lib/drawdown";
import { formatPct } from "../../lib/format";
import type { EquityPoint } from "../../types/api";

interface DrawdownChartProps {
  equityCurve: EquityPoint[];
}

export function DrawdownChart({ equityCurve }: DrawdownChartProps) {
  const data = deriveDrawdown(equityCurve);

  return (
    <div className="h-48 w-full">
      <ResponsiveContainer>
        <AreaChart data={data} margin={{ top: 8, right: 16, bottom: 8, left: 8 }}>
          <defs>
            <linearGradient id="ddFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#f43f5e" stopOpacity={0.1} />
              <stop offset="100%" stopColor="#f43f5e" stopOpacity={0.5} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="date" tick={{ fontSize: 11, fill: "#94a3b8" }} minTickGap={40} />
          <YAxis
            tick={{ fontSize: 11, fill: "#94a3b8" }}
            width={56}
            tickFormatter={(v) => formatPct(v, 0)}
            domain={["auto", 0]}
          />
          <Tooltip
            contentStyle={{
              background: "#0f172a",
              border: "1px solid #334155",
              borderRadius: 8,
              fontSize: 12,
            }}
            formatter={(v: number) => formatPct(v)}
          />
          <Area
            type="monotone"
            dataKey="drawdown"
            name="Drawdown"
            stroke="#f43f5e"
            strokeWidth={1.5}
            fill="url(#ddFill)"
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
