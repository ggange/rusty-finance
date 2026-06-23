import { MetricCard } from "./MetricCard";
import { CorrelationHeatmap } from "./CorrelationHeatmap";
import { RollingVolatilityChart } from "./RollingVolatilityChart";
import { formatPct, formatNum, signClass } from "../../lib/format";
import type { RiskMetrics } from "../../types/api";

interface RiskPanelProps {
  risk: RiskMetrics;
}

export function RiskPanel({ risk }: RiskPanelProps) {
  return (
    <div className="space-y-6">
      {/* VaR / CVaR cards */}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <MetricCard
          label="VaR 95%"
          value={formatPct(risk.var_95)}
          valueClass={signClass(risk.var_95)}
          benchmark="1-day, historical"
        />
        <MetricCard
          label="CVaR 95%"
          value={formatPct(risk.cvar_95)}
          valueClass={signClass(risk.cvar_95)}
          benchmark="expected shortfall"
        />
        <MetricCard
          label="VaR 99%"
          value={formatPct(risk.var_99)}
          valueClass={signClass(risk.var_99)}
          benchmark="1-day, historical"
        />
        <MetricCard
          label="CVaR 99%"
          value={formatPct(risk.cvar_99)}
          valueClass={signClass(risk.cvar_99)}
          benchmark="expected shortfall"
        />
      </div>

      {/* Per-asset risk table */}
      {risk.symbols.length > 0 && (
        <div className="overflow-auto">
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-slate-700 text-left text-slate-400">
                <th className="pb-1 pr-4">Asset</th>
                <th className="pb-1 pr-4 text-right">Ann. Vol</th>
                <th className="pb-1 pr-4 text-right">β vs port</th>
                <th className="pb-1 text-right">Risk contribution</th>
              </tr>
            </thead>
            <tbody>
              {risk.symbols.map((sym, i) => (
                <tr key={sym} className="border-b border-slate-800">
                  <td className="py-1.5 pr-4 font-medium text-slate-200">{sym}</td>
                  <td className="py-1.5 pr-4 text-right tabular-nums text-slate-300">
                    {formatPct(risk.asset_volatility[i])}
                  </td>
                  <td className="py-1.5 pr-4 text-right tabular-nums">
                    <span className={signClass(risk.asset_beta[i])}>
                      {formatNum(risk.asset_beta[i])}
                    </span>
                  </td>
                  <td className="py-1.5 text-right tabular-nums text-slate-300">
                    {formatPct(risk.contribution_to_risk[i])}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Rolling volatility chart */}
      <div>
        <p className="mb-2 text-xs font-medium uppercase tracking-wide text-slate-500">
          Rolling volatility (21-day, annualized)
        </p>
        <RollingVolatilityChart data={risk.rolling_volatility} />
      </div>

      {/* Correlation heatmap */}
      <div>
        <p className="mb-2 text-xs font-medium uppercase tracking-wide text-slate-500">
          Return correlation
        </p>
        <CorrelationHeatmap symbols={risk.symbols} correlation={risk.correlation} />
      </div>
    </div>
  );
}
