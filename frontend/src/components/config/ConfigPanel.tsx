import { Panel } from "../ui/Panel";
import { AssetList } from "./AssetList";
import { CostInputs } from "./CostInputs";
import type { PortfolioForm } from "../../state/usePortfolioForm";
import type { Dataset, StrategyMeta } from "../../types/api";

interface ConfigPanelProps {
  strategies: StrategyMeta[];
  datasets: Dataset[];
  form: PortfolioForm;
  onRun: () => void;
  running: boolean;
  engineAvailable: boolean;
}

export function ConfigPanel({
  strategies,
  datasets,
  form,
  onRun,
  running,
  engineAvailable,
}: ConfigPanelProps) {
  const hasData = form.assets.some(
    (a) => a.source.kind !== "none" && a.candles.length > 0,
  );
  const canRun = hasData && !running && engineAvailable;

  return (
    <div className="space-y-4">
      <Panel title="Portfolio">
        <AssetList strategies={strategies} datasets={datasets} form={form} />
      </Panel>

      <Panel title="Costs">
        <CostInputs
          initialCash={form.initialCash}
          commission={form.commission}
          slippagePct={form.slippagePct}
          onInitialCash={form.setInitialCash}
          onCommission={form.setCommission}
          onSlippagePct={form.setSlippagePct}
        />
      </Panel>

      <button
        type="button"
        onClick={onRun}
        disabled={!canRun}
        className="w-full rounded-lg bg-sky-500 px-4 py-2.5 font-semibold text-white transition hover:bg-sky-400 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-500"
      >
        {running ? "Running…" : "Run backtest"}
      </button>

      {!engineAvailable && (
        <p className="text-center text-xs text-amber-400">
          Engine unavailable — run <code>maturin develop</code> in
          backtesting-py.
        </p>
      )}
      {!hasData && engineAvailable && (
        <p className="text-center text-xs text-slate-500">
          Pick a dataset or upload a CSV for at least one asset to run.
        </p>
      )}
    </div>
  );
}
