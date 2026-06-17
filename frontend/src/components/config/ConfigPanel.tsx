import { Panel } from "../ui/Panel";
import { DataSourceLoader } from "./DataSourceLoader";
import { StrategyPicker } from "./StrategyPicker";
import { CostInputs } from "./CostInputs";
import type { BacktestForm } from "../../state/useBacktestForm";
import type { StrategyMeta } from "../../types/api";

interface ConfigPanelProps {
  strategies: StrategyMeta[];
  form: BacktestForm;
  onRun: () => void;
  running: boolean;
  engineAvailable: boolean;
}

export function ConfigPanel({
  strategies,
  form,
  onRun,
  running,
  engineAvailable,
}: ConfigPanelProps) {
  const hasCandles = form.candles.length > 0;
  const canRun = hasCandles && !running && engineAvailable;

  return (
    <div className="space-y-4">
      <Panel title="Data source">
        <DataSourceLoader
          candles={form.candles}
          fileName={form.fileName}
          onCandles={form.setCandles}
        />
      </Panel>

      <Panel title="Strategy">
        <StrategyPicker
          strategies={strategies}
          strategyType={form.strategyType}
          current={form.currentStrategyMeta}
          params={form.params}
          onTypeChange={form.setStrategyType}
          onParam={form.setParam}
        />
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
      {!hasCandles && engineAvailable && (
        <p className="text-center text-xs text-slate-500">
          Load price data to enable the backtest.
        </p>
      )}
    </div>
  );
}
