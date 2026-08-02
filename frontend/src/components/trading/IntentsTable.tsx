import { formatCurrency, formatQty, signClass } from "../../lib/format";
import { EmptyState } from "../ui/EmptyState";
import { StatusPill } from "../ui/StatusPill";
import { Table, Td, Th, Tr } from "../ui/Table";
import type { IntentRow } from "../../types/api";

/**
 * The API records an outcome as either a bare status ("filled") or a status
 * with its cause ("rejected: position value 10,000.00 exceeds
 * max_position_value 100.00"). Split them so the verb can be a compact pill and
 * the cause stays readable prose — a guardrail reason is the useful part and
 * does not belong squeezed into a lozenge.
 */
function splitOutcome(status: string): { verb: string; detail: string | null } {
  const i = status.indexOf(":");
  if (i === -1) return { verb: status, detail: null };
  return { verb: status.slice(0, i).trim(), detail: status.slice(i + 1).trim() };
}

/**
 * A rejected intent is a guardrail doing its job, so it is shown in the same
 * list as accepted ones rather than filtered out — the rejections are the
 * evidence that risk limits are wired in.
 */
export function IntentsTable({ intents }: { intents: IntentRow[] }) {
  if (intents.length === 0) {
    return (
      <EmptyState
        message="No intents recorded."
        hint="Every decision the loop makes is logged here, including ones the guardrails blocked."
      />
    );
  }

  return (
    <Table
      head={
        <>
          <Th>When</Th>
          <Th>Symbol</Th>
          <Th>Signal</Th>
          <Th>Side</Th>
          <Th align="right">Qty</Th>
          <Th align="right">Price</Th>
          <Th align="right">Realized P&amp;L</Th>
          <Th>Outcome</Th>
        </>
      }
    >
      {intents.map((it) => {
        const rejected = it.status.startsWith("rejected");
        const { verb, detail } = splitOutcome(it.status);
        return (
          <Tr key={it.id}>
            <Td className="text-xs text-slate-500">
              {new Date(it.created_at).toLocaleString()}
            </Td>
            <Td className="font-medium">{it.symbol}</Td>
            <Td className="text-xs text-slate-400">{it.signal}</Td>
            <Td
              className={`font-medium ${
                it.side === "buy" ? "text-emerald-400" : "text-rose-400"
              }`}
            >
              {it.side}
            </Td>
            <Td align="right" numeric>
              {formatQty(it.qty)}
            </Td>
            <Td align="right" numeric>
              {formatCurrency(it.price)}
            </Td>
            <Td align="right" numeric className={signClass(it.realized_pnl)}>
              {it.realized_pnl === null ? "—" : formatCurrency(it.realized_pnl)}
            </Td>
            <Td>
              <StatusPill tone={rejected ? "bad" : "ok"}>{verb}</StatusPill>
              {detail && (
                <p className="mt-1 text-xs text-slate-400">{detail}</p>
              )}
            </Td>
          </Tr>
        );
      })}
    </Table>
  );
}
