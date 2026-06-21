import { api } from "../../lib/apiClient";
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
  const config = run.config as { strategy?: { type?: string } };
  return config.strategy?.type ?? "Backtest";
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
        <button
          onClick={refresh}
          disabled={loading}
          className="rounded px-2 py-0.5 text-xs text-slate-400 hover:bg-slate-700 hover:text-slate-200 disabled:opacity-40"
        >
          {loading ? "…" : "Refresh"}
        </button>
      </div>

      {runs.length === 0 && !loading && (
        <p className="text-xs text-slate-500">No runs yet.</p>
      )}

      <ul className="flex flex-col gap-1">
        {runs.map((run) => (
          <li key={run.id}>
            <button
              onClick={() => void handleClick(run)}
              className="w-full rounded-md border border-slate-700 bg-slate-800/60 px-3 py-2 text-left hover:border-slate-600 hover:bg-slate-700/60"
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
        ))}
      </ul>
    </div>
  );
}
