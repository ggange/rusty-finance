import { Panel } from "../ui/Panel";
import { AssetList } from "./AssetList";
import { CostInputs } from "./CostInputs";
import { Field } from "../ui/Field";
import type { PortfolioForm } from "../../state/usePortfolioForm";
import type { Dataset, RebalanceConfig, StrategyMeta } from "../../types/api";

const selectClass =
  "w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none";

function RebalancingControls({
  value,
  onChange,
}: {
  value: RebalanceConfig | null;
  onChange: (v: RebalanceConfig | null) => void;
}) {
  const frequency = value?.frequency.kind ?? "none";
  const threshold = value?.frequency.kind === "threshold" ? value.frequency.threshold : 0.05;

  function handleFrequencyChange(kind: string) {
    if (kind === "none") {
      onChange(null);
    } else if (kind === "monthly" || kind === "quarterly") {
      onChange({ frequency: { kind } });
    } else if (kind === "threshold") {
      onChange({ frequency: { kind: "threshold", threshold } });
    }
  }

  return (
    <div className="space-y-3">
      <Field label="Frequency" htmlFor="rebalance-freq">
        <select
          id="rebalance-freq"
          value={frequency}
          onChange={(e) => handleFrequencyChange(e.target.value)}
          className={selectClass}
        >
          <option value="none">None</option>
          <option value="monthly">Monthly</option>
          <option value="quarterly">Quarterly</option>
          <option value="threshold">Drift threshold</option>
        </select>
      </Field>
      {frequency === "threshold" && (
        <Field label="Max drift (%)" htmlFor="rebalance-threshold" hint="Rebalance when any asset drifts by this much from its target weight">
          <input
            id="rebalance-threshold"
            type="number"
            min={0.1}
            max={50}
            step={0.5}
            value={threshold * 100}
            onChange={(e) =>
              onChange({ frequency: { kind: "threshold", threshold: Number(e.target.value) / 100 } })
            }
            className={selectClass}
          />
        </Field>
      )}
    </div>
  );
}

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

      <Panel title="Benchmark">
        <Field label="External benchmark" htmlFor="benchmark-symbol" hint="Overlay a buy-and-hold series on the equity chart">
          <select
            id="benchmark-symbol"
            value={form.benchmarkSymbol ?? ""}
            onChange={(e) => form.setBenchmarkSymbol(e.target.value || null)}
            className="w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none"
          >
            <option value="">None (internal buy &amp; hold only)</option>
            {datasets.map((d) => (
              <option key={d.name} value={d.name}>
                {d.symbol}
              </option>
            ))}
          </select>
        </Field>
      </Panel>

      <Panel title="Rebalancing">
        <RebalancingControls
          value={form.rebalanceConfig}
          onChange={form.setRebalanceConfig}
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
