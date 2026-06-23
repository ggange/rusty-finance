import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { formatPct } from "../../lib/format";
import type { RollingPoint } from "../../types/api";

interface RollingVolatilityChartProps {
  data: RollingPoint[];
}

export function RollingVolatilityChart({ data }: RollingVolatilityChartProps) {
  if (data.length === 0) {
    return (
      <p className="text-xs text-slate-500">
        Not enough data for rolling volatility (needs &gt;21 bars).
      </p>
    );
  }

  return (
    <div className="h-48 w-full">
      <ResponsiveContainer>
        <AreaChart data={data} margin={{ top: 8, right: 16, bottom: 8, left: 8 }}>
          <defs>
            <linearGradient id="rvFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#6366f1" stopOpacity={0.3} />
              <stop offset="100%" stopColor="#6366f1" stopOpacity={0.05} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="date" tick={{ fontSize: 11, fill: "#94a3b8" }} minTickGap={40} />
          <YAxis
            tick={{ fontSize: 11, fill: "#94a3b8" }}
            width={56}
            tickFormatter={(v) => formatPct(v, 1)}
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
            dataKey="value"
            name="Rolling vol (ann.)"
            stroke="#6366f1"
            strokeWidth={1.5}
            fill="url(#rvFill)"
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
