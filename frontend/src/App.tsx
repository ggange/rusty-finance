import { useState } from "react";
import { ConfigPanel } from "./components/config/ConfigPanel";
import { ResultsPanel } from "./components/results/ResultsPanel";
import { useStrategies } from "./hooks/useStrategies";
import { useBacktest } from "./hooks/useBacktest";
import { useBacktestForm } from "./state/useBacktestForm";
import type { Candle } from "./types/api";

function HealthBadge({
  engine,
}: {
  engine: "available" | "unavailable" | null;
}) {
  const label =
    engine === "available"
      ? "engine: available"
      : engine === "unavailable"
        ? "engine: unavailable"
        : "engine: …";
  const color =
    engine === "available"
      ? "bg-emerald-500/15 text-emerald-300 border-emerald-600/40"
      : engine === "unavailable"
        ? "bg-amber-500/15 text-amber-300 border-amber-600/40"
        : "bg-slate-700/40 text-slate-400 border-slate-600";
  return (
    <span className={`rounded-full border px-3 py-1 text-xs font-medium ${color}`}>
      {label}
    </span>
  );
}

export default function App() {
  const { strategies, health, loading, error: loadError } = useStrategies();
  const form = useBacktestForm(strategies);
  const { result, status, error, run } = useBacktest();

  // Snapshot the data the run used, so charts stay consistent with the result
  // even if the user edits the form afterward.
  const [ranWith, setRanWith] = useState<{
    candles: Candle[];
    initialCash: number;
  }>({ candles: [], initialCash: 10_000 });

  const engineAvailable = health?.engine === "available";

  function handleRun() {
    const req = form.buildRequest();
    if (!req) return;
    setRanWith({ candles: req.candles, initialCash: req.initial_cash });
    void run(req);
  }

  return (
    <div className="min-h-screen bg-slate-900 text-slate-100">
      <header className="border-b border-slate-800 bg-slate-900/80 px-6 py-4">
        <div className="mx-auto flex max-w-7xl items-center justify-between">
          <div>
            <h1 className="text-lg font-semibold">Rusty Finance · Backtester</h1>
            <p className="text-xs text-slate-500">
              Strategy backtesting on a native Rust engine
            </p>
          </div>
          <HealthBadge engine={health ? health.engine : null} />
        </div>
      </header>

      <main className="mx-auto max-w-7xl px-6 py-6">
        {loadError ? (
          <div className="rounded-lg bg-rose-950/60 px-4 py-3 text-sm text-rose-300">
            Failed to load from the API: {loadError}. Is the backend running on
            :8000?
          </div>
        ) : loading ? (
          <p className="text-slate-400">Loading strategies…</p>
        ) : (
          <div className="grid gap-6 lg:grid-cols-[360px_1fr]">
            <ConfigPanel
              strategies={strategies}
              form={form}
              onRun={handleRun}
              running={status === "loading"}
              engineAvailable={!!engineAvailable}
            />
            <ResultsPanel
              status={status}
              error={error}
              result={result}
              candles={ranWith.candles}
              initialCash={ranWith.initialCash}
            />
          </div>
        )}
      </main>
    </div>
  );
}
