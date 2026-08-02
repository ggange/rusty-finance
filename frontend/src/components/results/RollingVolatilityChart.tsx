import {
  Area,
  AreaChart,
  CartesianGrid,
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
  SERIES,
  TOOLTIP_STYLE,
} from "../../lib/chartTheme";
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
        <AreaChart data={data} margin={CHART_MARGIN}>
          <defs>
            <linearGradient id="rvFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={SERIES.volatility} stopOpacity={0.3} />
              <stop offset="100%" stopColor={SERIES.volatility} stopOpacity={0.05} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray={GRID_DASH} stroke={GRID} />
          <XAxis dataKey="date" tick={AXIS_TICK} minTickGap={40} />
          <YAxis tick={AXIS_TICK} width={56} tickFormatter={(v) => formatPct(v, 1)} />
          <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v: number) => formatPct(v)} />
          <Area
            type="monotone"
            dataKey="value"
            name="Rolling vol (ann.)"
            stroke={SERIES.volatility}
            strokeWidth={1.5}
            fill="url(#rvFill)"
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
