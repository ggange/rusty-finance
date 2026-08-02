import { formatPct } from "../../lib/format";
import { Panel } from "../ui/Panel";
import { StatusPill } from "../ui/StatusPill";
import { Table, Td, Th, Tr } from "../ui/Table";
import type { WeightSnapshot } from "../../types/api";

/** A bar rendered inside the cell, so a weight reads at a glance. */
function WeightBar({ value }: { value: number }) {
  return (
    <div className="flex items-center justify-end gap-2">
      <div className="h-1.5 w-16 overflow-hidden rounded-full bg-slate-700">
        <div
          className="h-full rounded-full bg-sky-400"
          style={{ width: `${Math.max(0, Math.min(1, value)) * 100}%` }}
        />
      </div>
      <span className="w-14 text-right tabular-nums">{formatPct(value, 1)}</span>
    </div>
  );
}

interface SolvedWeightsPanelProps {
  symbols: string[];
  history: WeightSnapshot[];
}

/**
 * The allocation a solving weight policy arrived at.
 *
 * Shows the weights currently in force plus how many times they were re-solved,
 * so a dynamic run is visibly different from a static one. Risk contribution is
 * displayed alongside because for risk parity it is the thing being equalized —
 * seeing it flat is how you confirm the objective did what it claims.
 */
export function SolvedWeightsPanel({ symbols, history }: SolvedWeightsPanelProps) {
  if (history.length === 0) return null;

  const latest = history[history.length - 1];
  const first = history[0];
  const dynamic = history.length > 1;

  return (
    <Panel title={dynamic ? "Allocation (latest solve)" : "Solved allocation"}>
      <div className="mb-3 flex flex-wrap items-center gap-2 text-xs text-slate-400">
        <StatusPill tone="info">
          {dynamic ? `${history.length} solves` : "solved once"}
        </StatusPill>
        <span>
          {dynamic ? `${first.date} → ${latest.date}` : `effective ${latest.date}`}
        </span>
        {latest.expected_volatility !== undefined && (
          <span>
            · predicted volatility{" "}
            <span className="text-slate-200 tabular-nums">
              {formatPct(latest.expected_volatility, 1)}
            </span>
          </span>
        )}
      </div>

      <Table
        maxHeight="max-h-72"
        head={
          <>
            <Th>Asset</Th>
            <Th align="right">Weight</Th>
            <Th align="right">Risk share</Th>
          </>
        }
      >
        {latest.weights.map((w, i) => (
          <Tr key={symbols[i] ?? i}>
            <Td className="font-medium">{symbols[i] ?? `Asset ${i + 1}`}</Td>
            <Td align="right">
              <WeightBar value={w} />
            </Td>
            <Td align="right" numeric>
              {latest.risk_contribution
                ? formatPct(latest.risk_contribution[i], 1)
                : "—"}
            </Td>
          </Tr>
        ))}
      </Table>

      <p className="mt-3 text-xs text-slate-500">
        {dynamic
          ? "Weights were re-solved at each rebalance date from a trailing window, so no solve saw data from after its own date."
          : "Solved once from the warm-up window, then held. The solve saw only the warm-up, never later data."}{" "}
        Predicted volatility is what the optimizer expected from its estimation
        window — not what the run realized.
      </p>
    </Panel>
  );
}
