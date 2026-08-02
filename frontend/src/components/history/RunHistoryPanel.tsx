import { api } from "../../lib/apiClient";
import { Button } from "../ui/Button";
import type { RunDetail, RunListItem } from "../../types/api";
import type { RunHistory } from "../../hooks/useRunHistory";

interface RunHistoryPanelProps {
  history: RunHistory;
  onLoad: (run: RunDetail) => void;
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function runLabel(run: RunListItem): string {
  if (run.kind === "portfolio") {
    const config = run.config as { assets?: { symbol?: string }[] };
    const symbols = config.assets?.map((a) => a.symbol).filter(Boolean) ?? [];
    return symbols.length > 0 ? symbols.join(", ") : "Portfolio";
  }
  if (run.kind === "scheduled_tick") {
    const config = run.config as { plan_ids?: string[] };
    const plans = config?.plan_ids ?? [];
    return plans.length > 0 ? `Tick · ${plans.join(", ")}` : "Scheduled tick";
  }
  const config = run.config as { strategy?: { type?: string } };
  return config.strategy?.type ?? "Backtest";
}

/**
 * Only portfolio runs can be reloaded into the backtest view. Tick summaries
 * live in the Trading tab and backtest runs use a different result shape, so
 * they are shown but not clickable — better than a button that does nothing.
 */
function isRestorable(run: RunListItem): boolean {
  return run.kind === "portfolio";
}

export function RunHistoryPanel({ history, onLoad }: RunHistoryPanelProps) {
  const { runs, loading, refresh } = history;

  async function handleClick(run: RunListItem) {
    try {
      const detail = await api.run(run.id);
      onLoad(detail);
    } catch {
      // silently ignore — the item stays clickable for a retry
    }
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-slate-300">Run History</h2>
        <Button variant="ghost" size="sm" onClick={refresh} disabled={loading}>
          {loading ? "…" : "Refresh"}
        </Button>
      </div>

      {runs.length === 0 && !loading && (
        <p className="text-xs text-slate-500">No runs yet.</p>
      )}

      <ul className="flex flex-col gap-1">
        {runs.map((run) => {
          const restorable = isRestorable(run);
          return (
            <li key={run.id}>
              <button
                type="button"
                onClick={() => void handleClick(run)}
                disabled={!restorable}
                title={restorable ? "Load this run" : `${run.kind} runs can't be reloaded here`}
                className={`w-full rounded-md border border-slate-700 bg-slate-800/60 px-3 py-2 text-left ${
                  restorable
                    ? "hover:border-slate-600 hover:bg-slate-700/60"
                    : "cursor-default opacity-60"
                }`}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate text-xs font-medium text-slate-200">
                    {runLabel(run)}
                  </span>
                  <span className="shrink-0 rounded-full bg-slate-700 px-1.5 py-0.5 text-[10px] text-slate-400">
                    {run.kind}
                  </span>
                </div>
                <div className="mt-0.5 text-[10px] text-slate-500">
                  #{run.id} · {formatDate(run.created_at)}
                </div>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
