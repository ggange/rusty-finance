import { useEffect, useState } from "react";
import { api } from "../../lib/apiClient";
import { formatCurrency, formatInt } from "../../lib/format";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Input } from "../ui/Input";
import { Panel } from "../ui/Panel";
import { Select } from "../ui/Select";
import type { LimitsForPlan, RiskLimits, RiskLimitsRow } from "../../types/api";

/** The API's global fallback row id (db.GLOBAL_LIMITS). */
export const GLOBAL_LIMITS = "__global__";

type Draft = Record<keyof RiskLimits, string>;

const EMPTY: Draft = {
  max_position_value: "",
  max_daily_loss: "",
  max_daily_orders: "",
};

function toDraft(row: RiskLimitsRow | null): Draft {
  if (!row) return EMPTY;
  return {
    max_position_value: row.max_position_value?.toString() ?? "",
    max_daily_loss: row.max_daily_loss?.toString() ?? "",
    max_daily_orders: row.max_daily_orders?.toString() ?? "",
  };
}

/** Blank means "no limit for this field", which the API expects as null. */
function toNumber(s: string): number | null {
  const t = s.trim();
  if (t === "") return null;
  const n = Number(t);
  return Number.isFinite(n) && n > 0 ? n : null;
}

function describe(value: number | null, kind: "money" | "count"): string {
  if (value === null) return "unlimited";
  return kind === "money" ? formatCurrency(value, 0) : formatInt(value);
}

interface RiskLimitsFormProps {
  planId: string;
  limits: LimitsForPlan | null;
  busy: boolean;
  act: <T>(fn: () => Promise<T>) => Promise<T | null>;
}

/**
 * Edit the global fallback row or one plan's overrides, and show the resulting
 * effective limits. The merge is field-by-field (api/risk.py resolve_limits),
 * which is easy to get wrong from memory — so it is displayed, not assumed.
 */
export function RiskLimitsForm({ planId, limits, busy, act }: RiskLimitsFormProps) {
  const [scope, setScope] = useState<"global" | "plan">("global");
  const [draft, setDraft] = useState<Draft>(EMPTY);

  const targetId = scope === "global" ? GLOBAL_LIMITS : planId;
  const storedRow = scope === "global" ? (limits?.global ?? null) : (limits?.plan ?? null);

  // Re-seed whenever the scope, plan, or fetched row changes.
  useEffect(() => {
    setDraft(toDraft(storedRow));
  }, [storedRow, scope, planId]);

  const eff = limits?.effective;

  async function save() {
    await act(() =>
      api.trade.setLimits({
        plan_id: targetId,
        max_position_value: toNumber(draft.max_position_value),
        max_daily_loss: toNumber(draft.max_daily_loss),
        max_daily_orders: toNumber(draft.max_daily_orders),
      }),
    );
  }

  async function clear() {
    await act(() => api.trade.deleteLimits(targetId));
  }

  return (
    <Panel title="Risk limits">
      <div className="space-y-3">
        <Field label="Applies to" htmlFor="limits-scope">
          <Select
            id="limits-scope"
            size="sm"
            value={scope}
            onChange={(e) => setScope(e.target.value as "global" | "plan")}
          >
            <option value="global">Global fallback</option>
            <option value="plan">Plan: {planId}</option>
          </Select>
        </Field>

        <Field
          label="Max position value ($)"
          htmlFor="limit-position"
          hint="Blank = unlimited. Required before a live broker will accept entries."
        >
          <Input
            id="limit-position"
            size="sm"
            type="number"
            min={0}
            value={draft.max_position_value}
            onChange={(e) => setDraft({ ...draft, max_position_value: e.target.value })}
          />
        </Field>

        <Field
          label="Max daily loss ($)"
          htmlFor="limit-loss"
          hint="Halts new entries once realized loss today reaches this"
        >
          <Input
            id="limit-loss"
            size="sm"
            type="number"
            min={0}
            value={draft.max_daily_loss}
            onChange={(e) => setDraft({ ...draft, max_daily_loss: e.target.value })}
          />
        </Field>

        <Field
          label="Max daily orders"
          htmlFor="limit-orders"
          hint="Runaway-loop guard"
        >
          <Input
            id="limit-orders"
            size="sm"
            type="number"
            min={0}
            step={1}
            value={draft.max_daily_orders}
            onChange={(e) => setDraft({ ...draft, max_daily_orders: e.target.value })}
          />
        </Field>

        <div className="flex gap-2">
          <Button className="flex-1" disabled={busy} loading={busy} onClick={() => void save()}>
            Save limits
          </Button>
          <Button
            variant="secondary"
            disabled={busy || !storedRow}
            onClick={() => void clear()}
            title={storedRow ? "Delete this row" : "Nothing stored for this scope"}
          >
            Clear
          </Button>
        </div>

        <div className="rounded-md border border-slate-700 bg-slate-900/60 p-3">
          <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-400">
            Effective for {planId}
          </p>
          <dl className="space-y-1 text-xs">
            <div className="flex justify-between gap-2">
              <dt className="text-slate-500">Max position</dt>
              <dd className="text-slate-200">
                {describe(eff?.max_position_value ?? null, "money")}
              </dd>
            </div>
            <div className="flex justify-between gap-2">
              <dt className="text-slate-500">Max daily loss</dt>
              <dd className="text-slate-200">{describe(eff?.max_daily_loss ?? null, "money")}</dd>
            </div>
            <div className="flex justify-between gap-2">
              <dt className="text-slate-500">Max daily orders</dt>
              <dd className="text-slate-200">
                {describe(eff?.max_daily_orders ?? null, "count")}
              </dd>
            </div>
          </dl>
          <p className="mt-2 text-[11px] text-slate-500">
            Plan overrides layer over the global row, field by field. Limits constrain
            entries only — exits are never blocked by a limit.
          </p>
        </div>
      </div>
    </Panel>
  );
}
