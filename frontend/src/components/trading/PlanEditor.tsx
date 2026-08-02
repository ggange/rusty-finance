import { useEffect, useState } from "react";
import { api } from "../../lib/apiClient";
import { formatCurrency } from "../../lib/format";
import { StrategyPicker } from "../config/StrategyPicker";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Input } from "../ui/Input";
import { Panel } from "../ui/Panel";
import { Select } from "../ui/Select";
import { StatusPill } from "../ui/StatusPill";
import type {
  Dataset,
  StrategyMeta,
  StrategyRequest,
  StrategyType,
  TradePlan,
  TradePlanItem,
} from "../../types/api";

interface DraftItem {
  id: string;
  dataset: string;
  strategyType: StrategyType | null;
  params: Record<string, number>;
  cashAllocation: number;
}

let seq = 0;
function newItem(dataset: string, strategies: StrategyMeta[]): DraftItem {
  const meta = strategies[0] ?? null;
  return {
    id: `item-${++seq}`,
    dataset,
    strategyType: meta?.type ?? null,
    params: Object.fromEntries((meta?.params ?? []).map((p) => [p.name, p.default])),
    cashAllocation: 10_000,
  };
}

/** Turn a draft row into the {type, ...params} shape the API expects. */
function toStrategyRequest(item: DraftItem): StrategyRequest | null {
  if (!item.strategyType) return null;
  return { type: item.strategyType, ...item.params } as unknown as StrategyRequest;
}

function fromPlan(plan: TradePlan, strategies: StrategyMeta[]): DraftItem[] {
  return plan.items.map((it) => {
    const { type, ...params } = it.strategy as unknown as Record<string, unknown> & {
      type: StrategyType;
    };
    const known = strategies.find((s) => s.type === type);
    return {
      id: `item-${++seq}`,
      dataset: it.dataset,
      strategyType: known ? type : (strategies[0]?.type ?? null),
      params: params as Record<string, number>,
      cashAllocation: it.cash_allocation,
    };
  });
}

interface PlanEditorProps {
  plans: TradePlan[];
  planId: string;
  onPlanId: (id: string) => void;
  datasets: Dataset[];
  strategies: StrategyMeta[];
  busy: boolean;
  act: <T>(fn: () => Promise<T>) => Promise<T | null>;
}

/**
 * Create, edit and delete the stored plans the scheduler runs unattended.
 * Datasets are validated server-side on save, so a bad symbol fails here rather
 * than at 16:30 with nobody watching.
 */
export function PlanEditor({
  plans,
  planId,
  onPlanId,
  datasets,
  strategies,
  busy,
  act,
}: PlanEditorProps) {
  const [items, setItems] = useState<DraftItem[]>([]);
  const [enabled, setEnabled] = useState(true);
  const [dirty, setDirty] = useState(false);

  const current = plans.find((p) => p.plan_id === planId) ?? null;

  // Load the selected plan into the draft, unless there are unsaved edits.
  useEffect(() => {
    if (dirty) return;
    if (current) {
      setItems(fromPlan(current, strategies));
      setEnabled(current.enabled);
    } else {
      setItems([]);
      setEnabled(true);
    }
  }, [current, strategies, dirty]);

  function update(id: string, patch: Partial<DraftItem>) {
    setDirty(true);
    setItems((prev) => prev.map((it) => (it.id === id ? { ...it, ...patch } : it)));
  }

  function changeStrategy(id: string, type: StrategyType) {
    const meta = strategies.find((s) => s.type === type);
    update(id, {
      strategyType: type,
      params: Object.fromEntries((meta?.params ?? []).map((p) => [p.name, p.default])),
    });
  }

  async function save() {
    const payload: TradePlanItem[] = [];
    for (const it of items) {
      const strategy = toStrategyRequest(it);
      if (!strategy || !it.dataset) continue;
      payload.push({
        dataset: it.dataset,
        strategy,
        cash_allocation: it.cashAllocation,
      });
    }
    if (payload.length === 0) return;
    const saved = await act(() =>
      api.trade.savePlan({ plan_id: planId, items: payload, enabled }),
    );
    if (saved) setDirty(false);
  }

  async function remove() {
    const ok = await act(() => api.trade.deletePlan(planId));
    if (ok) setDirty(false);
  }

  const totalCash = items.reduce((s, it) => s + (it.cashAllocation || 0), 0);
  const canSave = items.length > 0 && items.every((it) => it.dataset && it.strategyType);

  return (
    <Panel title="Trading plan">
      <div className="space-y-3">
        <div className="grid grid-cols-[1fr_auto] gap-2">
          <Field label="Plan" htmlFor="plan-select">
            <Select
              id="plan-select"
              size="sm"
              value={plans.some((p) => p.plan_id === planId) ? planId : ""}
              onChange={(e) => {
                setDirty(false);
                onPlanId(e.target.value || "default");
              }}
            >
              {!plans.some((p) => p.plan_id === planId) && (
                <option value="">{planId} (unsaved)</option>
              )}
              {plans.map((p) => (
                <option key={p.plan_id} value={p.plan_id}>
                  {p.plan_id}
                  {p.enabled ? "" : " (disabled)"}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Plan id" htmlFor="plan-id">
            <Input
              id="plan-id"
              size="sm"
              className="w-32"
              value={planId}
              onChange={(e) => onPlanId(e.target.value)}
            />
          </Field>
        </div>

        <label className="flex items-center gap-2 text-sm text-slate-300">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => {
              setDirty(true);
              setEnabled(e.target.checked);
            }}
            className="accent-sky-500"
          />
          Enabled — the scheduler runs this plan
        </label>

        {items.map((item, i) => (
          <div key={item.id} className="space-y-3 rounded-lg border border-slate-700 bg-slate-900/40 p-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-semibold uppercase tracking-wide text-slate-400">
                Position {i + 1}
              </span>
              <Button
                variant="ghost"
                size="sm"
                className="px-0 hover:text-rose-400"
                onClick={() => {
                  setDirty(true);
                  setItems((prev) => prev.filter((x) => x.id !== item.id));
                }}
                aria-label={`Remove position ${i + 1}`}
              >
                Remove
              </Button>
            </div>

            <div className="grid grid-cols-[1fr_auto] gap-2">
              <Field label="Dataset" htmlFor={`${item.id}-dataset`}>
                <Select
                  id={`${item.id}-dataset`}
                  size="sm"
                  value={item.dataset}
                  onChange={(e) => update(item.id, { dataset: e.target.value })}
                >
                  <option value="">Pick a dataset…</option>
                  {datasets.map((d) => (
                    <option key={d.name} value={d.name}>
                      {d.symbol}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="Cash ($)" htmlFor={`${item.id}-cash`}>
                <Input
                  id={`${item.id}-cash`}
                  size="sm"
                  className="w-28"
                  type="number"
                  min={1}
                  step={100}
                  value={Number.isFinite(item.cashAllocation) ? item.cashAllocation : ""}
                  onChange={(e) => update(item.id, { cashAllocation: Number(e.target.value) })}
                />
              </Field>
            </div>

            <StrategyPicker
              strategies={strategies}
              idPrefix={`${item.id}-`}
              strategyType={item.strategyType}
              current={strategies.find((s) => s.type === item.strategyType) ?? null}
              params={item.params}
              onTypeChange={(t) => changeStrategy(item.id, t)}
              onParam={(name, v) =>
                update(item.id, { params: { ...item.params, [name]: v } })
              }
            />
          </div>
        ))}

        <Button
          variant="ghost"
          className="w-full border border-dashed border-slate-600 text-sky-300 hover:bg-slate-700/40 hover:text-sky-200"
          onClick={() => {
            setDirty(true);
            setItems((prev) => [...prev, newItem(datasets[0]?.name ?? "", strategies)]);
          }}
        >
          + Add position
        </Button>

        {items.length > 0 && (
          <p className="text-center text-xs text-slate-500">
            Total allocation {formatCurrency(totalCash, 0)}
          </p>
        )}

        <div className="flex items-center gap-2">
          <Button
            className="flex-1"
            disabled={!canSave || busy}
            loading={busy}
            onClick={() => void save()}
          >
            {current ? "Save plan" : "Create plan"}
          </Button>
          <Button
            variant="secondary"
            disabled={!current || busy}
            onClick={() => void remove()}
          >
            Delete
          </Button>
        </div>

        {dirty && <StatusPill tone="warn">unsaved changes</StatusPill>}
      </div>
    </Panel>
  );
}
