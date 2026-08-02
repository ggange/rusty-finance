import { useState } from "react";
import { api } from "../../lib/apiClient";
import { Button } from "../ui/Button";
import { Panel } from "../ui/Panel";
import type { BrokerInfo, TickResult } from "../../types/api";

type Pending = "tick" | "cycle" | null;

interface RunControlsProps {
  planId: string;
  broker: BrokerInfo | null;
  killSwitchEngaged: boolean;
  busy: boolean;
  act: <T>(fn: () => Promise<T>) => Promise<T | null>;
  onTick: (tick: TickResult) => void;
}

/**
 * Manual triggers for the trading loop.
 *
 * Both of these submit real orders to whatever broker is configured, so each
 * one goes through an explicit confirm step and turns red when the broker is
 * live. There is no way to fire either in a single click.
 */
export function RunControls({
  planId,
  broker,
  killSwitchEngaged,
  busy,
  act,
  onTick,
}: RunControlsProps) {
  const [pending, setPending] = useState<Pending>(null);
  const [refreshData, setRefreshData] = useState(true);

  const live = broker?.is_live ?? false;
  const variant = live ? "danger" : "primary";

  async function runTick() {
    const res = await act(() => api.trade.tick({ plan_id: planId }));
    if (res) onTick(res);
    setPending(null);
  }

  async function runCycle() {
    await act(() => api.trade.scheduleRun(refreshData));
    setPending(null);
  }

  return (
    <Panel title="Run">
      <div className="space-y-3">
        <p className="rounded-md bg-slate-900/60 px-3 py-2 text-xs text-slate-400">
          These place orders on <span className="text-slate-200">{broker?.name ?? "…"}</span>
          {live && <span className="font-semibold text-rose-300"> — a LIVE venue</span>}.
        </p>

        {killSwitchEngaged && (
          <p className="rounded-md bg-rose-950/50 px-3 py-2 text-xs text-rose-300">
            Kill switch is engaged — the loop will run but every order will be blocked.
          </p>
        )}

        {pending === "tick" ? (
          <div className="space-y-2 rounded-md border border-slate-600 bg-slate-900/60 p-3">
            <p className="text-sm text-slate-200">
              Tick plan <span className="font-mono">{planId}</span> now?
            </p>
            <div className="flex gap-2">
              <Button variant={variant} size="sm" loading={busy} onClick={() => void runTick()}>
                Confirm tick
              </Button>
              <Button variant="ghost" size="sm" onClick={() => setPending(null)}>
                Cancel
              </Button>
            </div>
          </div>
        ) : (
          <Button
            variant="secondary"
            className="w-full"
            disabled={busy}
            onClick={() => setPending("tick")}
          >
            Tick this plan…
          </Button>
        )}

        {pending === "cycle" ? (
          <div className="space-y-2 rounded-md border border-slate-600 bg-slate-900/60 p-3">
            <p className="text-sm text-slate-200">
              Run the full scheduled cycle for every enabled plan?
            </p>
            <label className="flex items-center gap-2 text-xs text-slate-400">
              <input
                type="checkbox"
                checked={refreshData}
                onChange={(e) => setRefreshData(e.target.checked)}
                className="accent-sky-500"
              />
              Refresh market data first
            </label>
            <div className="flex gap-2">
              <Button variant={variant} size="sm" loading={busy} onClick={() => void runCycle()}>
                Confirm cycle
              </Button>
              <Button variant="ghost" size="sm" onClick={() => setPending(null)}>
                Cancel
              </Button>
            </div>
          </div>
        ) : (
          <Button
            variant="secondary"
            className="w-full"
            disabled={busy}
            onClick={() => setPending("cycle")}
          >
            Run scheduled cycle…
          </Button>
        )}
      </div>
    </Panel>
  );
}
