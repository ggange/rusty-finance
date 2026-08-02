import { useState } from "react";
import { IntentsTable } from "./IntentsTable";
import { KillSwitchControl } from "./KillSwitchControl";
import { OrdersTable } from "./OrdersTable";
import { PlanEditor } from "./PlanEditor";
import { PositionsTable } from "./PositionsTable";
import { ReconcilePanel } from "./ReconcilePanel";
import { RiskLimitsForm } from "./RiskLimitsForm";
import { RunControls } from "./RunControls";
import { SoakPanel } from "./SoakPanel";
import { StatusStrip } from "./StatusStrip";
import { Button } from "../ui/Button";
import { Panel } from "../ui/Panel";
import { Spinner } from "../ui/Spinner";
import type { TradeConsole } from "../../hooks/useTradeConsole";
import type { Dataset, StrategyMeta } from "../../types/api";

const VIEWS = [
  { key: "positions", label: "Positions" },
  { key: "orders", label: "Orders" },
  { key: "intents", label: "Intents" },
  { key: "soak", label: "Soak" },
  { key: "reconcile", label: "Reconcile" },
] as const;

type View = (typeof VIEWS)[number]["key"];

interface TradingConsoleProps {
  console: TradeConsole;
  datasets: Dataset[];
  strategies: StrategyMeta[];
  engineAvailable: boolean;
}

export function TradingConsole({
  console: c,
  datasets,
  strategies,
  engineAvailable,
}: TradingConsoleProps) {
  const [view, setView] = useState<View>("positions");

  const counts: Record<View, number | null> = {
    positions: c.positions.length,
    orders: c.orders.length,
    intents: c.intents.length,
    soak: null,
    reconcile: c.reconcile ? c.reconcile.drift.length : null,
  };

  if (c.status === "loading" && !c.broker) {
    return (
      <div className="flex justify-center py-24">
        <Spinner label="Loading trading state…" />
      </div>
    );
  }

  return (
    <div className="space-y-4 pt-4">
      {!engineAvailable && (
        <div className="rounded-lg border border-amber-600/50 bg-amber-950/40 px-4 py-3 text-sm text-amber-200">
          Engine unavailable — ticks will fail. Run <code>maturin develop</code> in
          backtesting-py.
        </div>
      )}

      {c.error && (
        <div className="rounded-lg bg-rose-950/60 px-4 py-3 text-sm text-rose-300">
          Failed to load trading state: {c.error}
        </div>
      )}

      <StatusStrip
        broker={c.broker}
        schedule={c.schedule}
        killSwitch={c.killSwitch}
        limits={c.limits}
        reconcile={c.reconcile}
      />

      {c.actionError && (
        <div className="flex items-start justify-between gap-3 rounded-lg bg-rose-950/60 px-4 py-3 text-sm text-rose-300">
          <span>{c.actionError}</span>
          <Button variant="ghost" size="sm" onClick={c.clearActionError}>
            Dismiss
          </Button>
        </div>
      )}

      <div className="grid gap-6 lg:grid-cols-[380px_1fr]">
        <div className="space-y-4">
          <PlanEditor
            plans={c.plans}
            planId={c.planId}
            onPlanId={c.setPlanId}
            datasets={datasets}
            strategies={strategies}
            busy={c.busy}
            act={c.act}
          />
          <RiskLimitsForm
            planId={c.planId}
            limits={c.limits}
            busy={c.busy}
            act={c.act}
          />
          <KillSwitchControl killSwitch={c.killSwitch} busy={c.busy} act={c.act} />
          <RunControls
            planId={c.planId}
            broker={c.broker}
            killSwitchEngaged={c.killSwitch?.engaged ?? false}
            busy={c.busy}
            act={c.act}
            onTick={c.applyTick}
          />
        </div>

        <Panel>
          <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-wrap gap-1">
              {VIEWS.map((v) => (
                <button
                  key={v.key}
                  type="button"
                  onClick={() => setView(v.key)}
                  className={`rounded-md px-3 py-1.5 text-sm font-medium transition ${
                    view === v.key
                      ? "bg-slate-700 text-slate-100"
                      : "text-slate-400 hover:text-slate-200"
                  }`}
                >
                  {v.label}
                  {counts[v.key] !== null && (
                    <span className="ml-1.5 text-xs text-slate-500">{counts[v.key]}</span>
                  )}
                </button>
              ))}
            </div>

            <div className="flex items-center gap-3">
              <label className="flex items-center gap-2 text-xs text-slate-400">
                <input
                  type="checkbox"
                  checked={c.polling}
                  onChange={(e) => c.setPolling(e.target.checked)}
                  className="accent-sky-500"
                />
                Auto-refresh
              </label>
              <Button
                variant="secondary"
                size="sm"
                disabled={c.status === "loading"}
                onClick={() => void c.refresh()}
              >
                Refresh
              </Button>
            </div>
          </div>

          {view === "positions" && <PositionsTable positions={c.positions} />}
          {view === "orders" && (
            <OrdersTable
              orders={c.orders}
              openOnly={c.openOnly}
              onOpenOnly={c.setOpenOnly}
            />
          )}
          {view === "intents" && <IntentsTable intents={c.intents} />}
          {view === "soak" && <SoakPanel soak={c.soak} broker={c.broker} />}
          {view === "reconcile" && <ReconcilePanel reconcile={c.reconcile} />}
        </Panel>
      </div>
    </div>
  );
}
