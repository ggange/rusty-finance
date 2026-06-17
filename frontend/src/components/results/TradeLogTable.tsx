import { formatCurrency, formatInt, signClass } from "../../lib/format";
import type { Trade } from "../../types/api";

interface TradeLogTableProps {
  trades: Trade[];
}

export function TradeLogTable({ trades }: TradeLogTableProps) {
  if (trades.length === 0) {
    return (
      <p className="text-sm text-slate-500">
        No trades were executed for this configuration.
      </p>
    );
  }

  return (
    <div className="max-h-80 overflow-auto rounded-lg border border-slate-700">
      <table className="w-full text-sm">
        <thead className="sticky top-0 bg-slate-900 text-xs uppercase tracking-wide text-slate-400">
          <tr>
            <th className="px-3 py-2 text-left">Date</th>
            <th className="px-3 py-2 text-left">Action</th>
            <th className="px-3 py-2 text-right">Shares</th>
            <th className="px-3 py-2 text-right">Price</th>
            <th className="px-3 py-2 text-right">Commission</th>
            <th className="px-3 py-2 text-right">Cash After</th>
            <th className="px-3 py-2 text-right">P&amp;L</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-800">
          {trades.map((t, i) => (
            <tr key={`${t.date}-${i}`} className="text-slate-200">
              <td className="px-3 py-1.5 text-left tabular-nums">{t.date}</td>
              <td
                className={`px-3 py-1.5 text-left font-medium ${
                  t.action === "Buy" ? "text-emerald-400" : "text-rose-400"
                }`}
              >
                {t.action}
              </td>
              <td className="px-3 py-1.5 text-right tabular-nums">
                {formatInt(t.shares)}
              </td>
              <td className="px-3 py-1.5 text-right tabular-nums">
                {formatCurrency(t.price)}
              </td>
              <td className="px-3 py-1.5 text-right tabular-nums">
                {formatCurrency(t.commission)}
              </td>
              <td className="px-3 py-1.5 text-right tabular-nums">
                {formatCurrency(t.cash_after)}
              </td>
              <td
                className={`px-3 py-1.5 text-right tabular-nums ${signClass(t.pnl)}`}
              >
                {t.pnl === null ? "—" : formatCurrency(t.pnl)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
