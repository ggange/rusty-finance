import { Panel } from "../ui/Panel";
import { AssetList } from "./AssetList";
import { CostInputs } from "./CostInputs";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import type { PortfolioForm } from "../../state/usePortfolioForm";
import type { Dataset, FillTiming, RebalanceConfig, StrategyMeta } from "../../types/api";

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
        <Select
          id="rebalance-freq"
          value={frequency}
          onChange={(e) => handleFrequencyChange(e.target.value)}
        >
          <option value="none">None</option>
          <option value="monthly">Monthly</option>
          <option value="quarterly">Quarterly</option>
          <option value="threshold">Drift threshold</option>
        </Select>
      </Field>
      {frequency === "threshold" && (
        <Field label="Max drift (%)" htmlFor="rebalance-threshold" hint="Rebalance when any asset drifts by this much from its target weight">
          <Input
            id="rebalance-threshold"
            type="number"
            min={0.1}
            max={50}
            step={0.5}
            value={threshold * 100}
            onChange={(e) =>
              onChange({ frequency: { kind: "threshold", threshold: Number(e.target.value) / 100 } })
            }
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
          <Select
            id="benchmark-symbol"
            value={form.benchmarkSymbol ?? ""}
            onChange={(e) => form.setBenchmarkSymbol(e.target.value || null)}
          >
            <option value="">None (internal buy &amp; hold only)</option>
            {datasets.map((d) => (
              <option key={d.name} value={d.name}>
                {d.symbol}
              </option>
            ))}
          </Select>
        </Field>
      </Panel>

      <Panel title="Rebalancing">
        <RebalancingControls
          value={form.rebalanceConfig}
          onChange={form.setRebalanceConfig}
        />
      </Panel>

      <Panel title="Execution">
        <Field label="Fill timing" htmlFor="fill-timing" hint="Next open = realistic (default); Close = legacy/lookahead-light">
          <Select
            id="fill-timing"
            value={form.fillTiming}
            onChange={(e) => form.setFillTiming(e.target.value as FillTiming)}
          >
            <option value="next_open">Next open (realistic)</option>
            <option value="close">Close (legacy)</option>
          </Select>
        </Field>
      </Panel>

      <Button
        onClick={onRun}
        disabled={!canRun}
        loading={running}
        className="w-full rounded-lg py-2.5 font-semibold"
      >
        {running ? "Running…" : "Run backtest"}
      </Button>

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
