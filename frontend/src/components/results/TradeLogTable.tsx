import { formatCurrency, formatInt, signClass } from "../../lib/format";
import { Table, Td, Th, Tr } from "../ui/Table";
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
    <Table
      head={
        <>
          <Th>Date</Th>
          <Th>Action</Th>
          <Th align="right">Shares</Th>
          <Th align="right">Price</Th>
          <Th align="right">Commission</Th>
          <Th align="right">Cash After</Th>
          <Th align="right">P&amp;L</Th>
        </>
      }
    >
      {trades.map((t, i) => (
        <Tr key={`${t.date}-${i}`}>
          <Td numeric>{t.date}</Td>
          <Td
            className={`font-medium ${
              t.action === "Buy" ? "text-emerald-400" : "text-rose-400"
            }`}
          >
            {t.action}
          </Td>
          <Td align="right" numeric>
            {formatInt(t.shares)}
          </Td>
          <Td align="right" numeric>
            {formatCurrency(t.price)}
          </Td>
          <Td align="right" numeric>
            {formatCurrency(t.commission)}
          </Td>
          <Td align="right" numeric>
            {formatCurrency(t.cash_after)}
          </Td>
          <Td align="right" numeric className={signClass(t.pnl)}>
            {t.pnl === null ? "—" : formatCurrency(t.pnl)}
          </Td>
        </Tr>
      ))}
    </Table>
  );
}
