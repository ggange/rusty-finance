import { formatQty } from "../../lib/format";
import { EmptyState } from "../ui/EmptyState";
import { Table, Td, Th, Tr } from "../ui/Table";
import type { ReconcileResult } from "../../types/api";

/**
 * Ledger vs venue. Drift means our record of reality is wrong, which makes
 * every decision made from it suspect — so an empty drift list is the good
 * state and is presented as such, not as "no data".
 */
export function ReconcilePanel({ reconcile }: { reconcile: ReconcileResult | null }) {
  if (!reconcile) {
    return <EmptyState message="Reconciliation not loaded yet." />;
  }

  if (reconcile.in_sync) {
    return (
      <div className="rounded-lg border border-emerald-700/40 bg-emerald-950/30 px-4 py-6 text-center">
        <p className="text-sm font-medium text-emerald-300">
          Ledger matches {reconcile.broker}
        </p>
        <p className="mt-1 text-xs text-emerald-200/70">
          {reconcile.checked} symbol{reconcile.checked === 1 ? "" : "s"} checked, no drift.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="rounded-lg border border-rose-600/50 bg-rose-950/40 px-4 py-3 text-sm text-rose-200">
        {reconcile.drift.length} symbol{reconcile.drift.length === 1 ? "" : "s"} drifting
        against {reconcile.broker}. Investigate before trusting any further tick.
      </div>
      <Table
        head={
          <>
            <Th>Symbol</Th>
            <Th align="right">Ledger qty</Th>
            <Th align="right">Broker qty</Th>
            <Th align="right">Delta</Th>
          </>
        }
      >
        {reconcile.drift.map((d) => (
          <Tr key={d.symbol}>
            <Td className="font-medium">{d.symbol}</Td>
            <Td align="right" numeric>
              {formatQty(d.ledger_qty)}
            </Td>
            <Td align="right" numeric>
              {formatQty(d.broker_qty)}
            </Td>
            <Td align="right" numeric className="text-rose-300">
              {d.delta > 0 ? "+" : ""}
              {formatQty(d.delta)}
            </Td>
          </Tr>
        ))}
      </Table>
    </div>
  );
}
