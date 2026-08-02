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
        <AreaChart data={data} margin={CHART_MARGIN}>
          <defs>
            <linearGradient id="ddFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={SERIES.negative} stopOpacity={0.1} />
              <stop offset="100%" stopColor={SERIES.negative} stopOpacity={0.5} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray={GRID_DASH} stroke={GRID} />
          <XAxis dataKey="date" tick={AXIS_TICK} minTickGap={40} />
          <YAxis
            tick={AXIS_TICK}
            width={56}
            tickFormatter={(v) => formatPct(v, 0)}
            domain={["auto", 0]}
          />
          <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v: number) => formatPct(v)} />
          <Area
            type="monotone"
            dataKey="drawdown"
            name="Drawdown"
            stroke={SERIES.negative}
            strokeWidth={1.5}
            fill="url(#ddFill)"
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
