import { useState } from "react";
import { ConfigPanel } from "./components/config/ConfigPanel";
import { PortfolioResultsPanel } from "./components/results/PortfolioResultsPanel";
import type { RanAsset } from "./components/results/PortfolioResultsPanel";
import { useStrategies } from "./hooks/useStrategies";
import { useDatasets } from "./hooks/useDatasets";
import { usePortfolio } from "./hooks/usePortfolio";
import { usePortfolioForm } from "./state/usePortfolioForm";

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
  const { datasets } = useDatasets();
  const form = usePortfolioForm(strategies);
  const { result, status, error, run } = usePortfolio();

  // Snapshot the candles each asset ran with, so per-asset charts stay
  // consistent with the result even if the form is edited afterward. The filter
  // here mirrors buildRequest(), so the order matches result.assets.
  const [ranAssets, setRanAssets] = useState<RanAsset[]>([]);

  const engineAvailable = health?.engine === "available";

  function handleRun() {
    const req = form.buildRequest();
    if (!req) return;
    const ready = form.assets.filter(
      (a) => a.source.kind !== "none" && a.candles.length > 0,
    );
    setRanAssets(
      ready.map((a) => ({ symbol: a.symbol || "ASSET", candles: a.candles })),
    );
    void run(req);
  }

  return (
    <div className="min-h-screen bg-slate-900 text-slate-100">
      <header className="border-b border-slate-800 bg-slate-900/80 px-6 py-4">
        <div className="mx-auto flex max-w-7xl items-center justify-between">
          <div>
            <h1 className="text-lg font-semibold">Rusty Finance · Backtester</h1>
            <p className="text-xs text-slate-500">
              Multi-asset portfolio backtesting on a native Rust engine
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
          <div className="grid gap-6 lg:grid-cols-[380px_1fr]">
            <ConfigPanel
              strategies={strategies}
              datasets={datasets}
              form={form}
              onRun={handleRun}
              running={status === "loading"}
              engineAvailable={!!engineAvailable}
            />
            <PortfolioResultsPanel
              status={status}
              error={error}
              result={result}
              ranAssets={ranAssets}
            />
          </div>
        )}
      </main>
    </div>
  );
}
