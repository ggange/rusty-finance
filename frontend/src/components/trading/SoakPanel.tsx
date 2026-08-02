import { formatCurrency, formatInt, formatNum, formatPct, signClass } from "../../lib/format";
import { EmptyState } from "../ui/EmptyState";
import type { BrokerInfo, SoakReport } from "../../types/api";

function Stat({
  label,
  value,
  className = "",
  hint,
}: {
  label: string;
  value: string;
  className?: string;
  hint?: string;
}) {
  return (
    <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-3">
      <p className="text-[10px] uppercase tracking-wide text-slate-500">{label}</p>
      <p className={`mt-1 text-lg font-semibold tabular-nums ${className}`}>{value}</p>
      {hint && <p className="mt-0.5 text-[11px] text-slate-500">{hint}</p>}
    </div>
  );
}

interface SoakPanelProps {
  soak: SoakReport | null;
  broker: BrokerInfo | null;
}

/**
 * Paper-soak evidence. The mean slippage number is only meaningful against a
 * real venue — on the simulator it reproduces the configured value, so that
 * caveat is stated inline rather than left for the reader to infer.
 */
export function SoakPanel({ soak, broker }: SoakPanelProps) {
  if (!soak) return <EmptyState message="Soak report not loaded yet." />;

  if (soak.orders === 0) {
    return (
      <EmptyState
        message="No orders submitted yet."
        hint="Fill rate and slippage need at least one order to measure."
      />
    );
  }

  const slip = soak.slippage_bps;
  const simulated = !broker?.is_live;

  return (
    <div className="space-y-3">
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <Stat label="Orders" value={formatInt(soak.orders)} />
        <Stat
          label="Filled"
          value={formatInt(soak.filled)}
          hint={`${formatInt(soak.rejected)} rejected`}
        />
        <Stat
          label="Fill rate"
          value={soak.fill_rate === null ? "—" : formatPct(soak.fill_rate, 1)}
          hint="filled qty / requested qty"
        />
        <Stat
          label="Mean slippage"
          value={slip.mean === null ? "—" : `${formatNum(slip.mean, 2)} bps`}
          className={slip.mean !== null && slip.mean > 0 ? "text-amber-300" : "text-emerald-400"}
          hint={`${formatInt(slip.samples)} sample${slip.samples === 1 ? "" : "s"}`}
        />
        <Stat
          label="Worst slippage"
          value={slip.worst === null ? "—" : `${formatNum(slip.worst, 2)} bps`}
          className={slip.worst !== null && slip.worst > 0 ? "text-amber-300" : "text-emerald-400"}
        />
        <Stat
          label="Realized P&L"
          value={formatCurrency(soak.realized_pnl)}
          className={signClass(soak.realized_pnl)}
        />
      </div>

      <p className="text-xs text-slate-500">{slip.note}.</p>

      {simulated && (
        <p className="rounded-md border border-amber-700/40 bg-amber-950/30 px-3 py-2 text-xs text-amber-200/90">
          These fills come from a simulator, so the slippage figure reproduces the{" "}
          {broker ? `${(broker.slippage * 10_000).toFixed(0)} bps` : "configured"} value you
          set — it is not a market telling you something. This validates loop stability,
          not execution quality.
        </p>
      )}
    </div>
  );
}
