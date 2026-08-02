import { formatCurrency, formatQty, formatStrategy } from "../../lib/format";
import { EmptyState } from "../ui/EmptyState";
import { Table, Td, Th, Tr } from "../ui/Table";
import type { PositionRow } from "../../types/api";

export function PositionsTable({ positions }: { positions: PositionRow[] }) {
  if (positions.length === 0) {
    return (
      <EmptyState
        message="Flat — no open positions."
        hint="Positions appear here once a tick produces a fill."
      />
    );
  }

  return (
    <Table
      head={
        <>
          <Th>Symbol</Th>
          <Th>Strategy</Th>
          <Th align="right">Qty</Th>
          <Th align="right">Avg price</Th>
          <Th align="right">Cost basis</Th>
          <Th>Updated</Th>
        </>
      }
    >
      {positions.map((p) => (
        <Tr key={`${p.plan_id}-${p.symbol}-${p.strategy}`}>
          <Td className="font-medium">{p.symbol}</Td>
          <Td className="text-xs text-slate-400">{formatStrategy(p.strategy)}</Td>
          <Td align="right" numeric>
            {formatQty(p.qty)}
          </Td>
          <Td align="right" numeric>
            {formatCurrency(p.avg_price)}
          </Td>
          <Td align="right" numeric>
            {formatCurrency(p.qty * p.avg_price)}
          </Td>
          <Td className="text-xs text-slate-500">
            {new Date(p.updated_at).toLocaleString()}
          </Td>
        </Tr>
      ))}
    </Table>
  );
}
