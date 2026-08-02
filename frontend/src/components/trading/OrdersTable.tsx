import { formatCurrency, formatQty } from "../../lib/format";
import { EmptyState } from "../ui/EmptyState";
import { StatusPill, orderTone } from "../ui/StatusPill";
import { Table, Td, Th, Tr } from "../ui/Table";
import type { OrderRow } from "../../types/api";

interface OrdersTableProps {
  orders: OrderRow[];
  openOnly: boolean;
  onOpenOnly: (v: boolean) => void;
}

export function OrdersTable({ orders, openOnly, onOpenOnly }: OrdersTableProps) {
  return (
    <div className="space-y-3">
      <label className="flex items-center gap-2 text-xs text-slate-400">
        <input
          type="checkbox"
          checked={openOnly}
          onChange={(e) => onOpenOnly(e.target.checked)}
          className="accent-sky-500"
        />
        Open orders only (accepted / partially filled)
      </label>

      {orders.length === 0 ? (
        <EmptyState
          message={openOnly ? "No open orders." : "No orders yet."}
          hint={openOnly ? "Everything submitted has reached a terminal state." : undefined}
        />
      ) : (
        <Table
          head={
            <>
              <Th>Submitted</Th>
              <Th>Symbol</Th>
              <Th>Side</Th>
              <Th align="right">Filled / Qty</Th>
              <Th align="right">Avg fill</Th>
              <Th>Status</Th>
              <Th>Reason</Th>
            </>
          }
        >
          {orders.map((o) => (
            <Tr key={o.id}>
              <Td className="text-xs text-slate-500">
                {new Date(o.created_at).toLocaleString()}
              </Td>
              <Td className="font-medium">{o.symbol}</Td>
              <Td
                className={`font-medium ${
                  o.side === "buy" ? "text-emerald-400" : "text-rose-400"
                }`}
              >
                {o.side}
              </Td>
              <Td align="right" numeric>
                {formatQty(o.filled_qty)} / {formatQty(o.qty)}
              </Td>
              <Td align="right" numeric>
                {o.avg_fill_price ? formatCurrency(o.avg_fill_price) : "—"}
              </Td>
              <Td>
                <StatusPill tone={orderTone(o.status)}>{o.status}</StatusPill>
              </Td>
              <Td className="text-xs text-slate-400">{o.reason ?? "—"}</Td>
            </Tr>
          ))}
        </Table>
      )}
    </div>
  );
}
