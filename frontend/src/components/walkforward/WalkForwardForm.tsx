import { useState, useEffect } from "react";
import { ParamRangeGrid } from "../config/ParamRangeGrid";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Input } from "../ui/Input";
import { Panel } from "../ui/Panel";
import { Select } from "../ui/Select";
import { METRIC_OPTIONS } from "../../lib/metrics";
import type {
  Dataset,
  FillTiming,
  ParamRange,
  StrategyMeta,
  StrategyType,
  WalkForwardRequest,
} from "../../types/api";

interface WalkForwardFormProps {
  strategies: StrategyMeta[];
  datasets: Dataset[];
  initialCash: number;
  commission: number;
  slippagePct: number;
  onRun: (req: WalkForwardRequest) => void;
  running: boolean;
  engineAvailable: boolean;
}

export function WalkForwardForm({
  strategies,
  datasets,
  initialCash,
  commission,
  slippagePct,
  onRun,
  running,
  engineAvailable,
}: WalkForwardFormProps) {
  const [dataset, setDataset] = useState(datasets[0]?.name ?? "");
  const [strategyType, setStrategyType] = useState<StrategyType>(strategies[0]?.type ?? "ma_ema");
  const [metric, setMetric] = useState("sharpe_ratio");
  const [nWindows, setNWindows] = useState(5);
  const [trainFrac, setTrainFrac] = useState(0.7);
  const [fillTiming, setFillTiming] = useState<FillTiming>("next_open");
  const [paramRanges, setParamRanges] = useState<Record<string, ParamRange>>({});

  const selectedStrategy = strategies.find((s) => s.type === strategyType);

  // Re-seed param ranges when strategy changes.
  useEffect(() => {
    if (!selectedStrategy) return;
    const ranges: Record<string, ParamRange> = {};
    for (const p of selectedStrategy.params) {
      if (p.type === "integer") {
        ranges[p.name] = { min: p.default, max: p.default * 3, step: p.default };
      } else {
        ranges[p.name] = { min: p.default, max: p.default * 2, step: p.step ?? 0.5 };
      }
    }
    setParamRanges(ranges);
  }, [strategyType]); // eslint-disable-line react-hooks/exhaustive-deps

  function setRange(name: string, field: keyof ParamRange, value: number) {
    setParamRanges((prev) => ({
      ...prev,
      [name]: { ...prev[name], [field]: value },
    }));
  }

  function handleSubmit() {
    if (!dataset || !strategyType) return;
    onRun({
      dataset,
      strategy_type: strategyType,
      param_ranges: paramRanges,
      n_windows: nWindows,
      train_frac: trainFrac,
      metric,
      initial_cash: initialCash,
      commission,
      slippage_pct: slippagePct,
      fill_timing: fillTiming,
    });
  }

  const canRun = !!dataset && !!strategyType && engineAvailable && !running;

  return (
    <div className="space-y-4">
      <Panel title="Walk-forward settings">
        <div className="space-y-3">
          <Field label="Dataset" htmlFor="wf-dataset">
            <Select
              id="wf-dataset"
              value={dataset}
              onChange={(e) => setDataset(e.target.value)}
            >
              {datasets.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.symbol}
                </option>
              ))}
            </Select>
          </Field>

          <Field label="Strategy" htmlFor="wf-strategy">
            <Select
              id="wf-strategy"
              value={strategyType}
              onChange={(e) => setStrategyType(e.target.value as StrategyType)}
            >
              {strategies.map((s) => (
                <option key={s.type} value={s.type}>
                  {s.name}
                </option>
              ))}
            </Select>
          </Field>

          <Field label="Optimise by" htmlFor="wf-metric">
            <Select
              id="wf-metric"
              value={metric}
              onChange={(e) => setMetric(e.target.value)}
            >
              {METRIC_OPTIONS.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </Select>
          </Field>

          <div className="grid grid-cols-2 gap-3">
            <Field label="Windows" htmlFor="wf-windows" hint="Number of rolling folds (≥ 2)">
              <Input
                id="wf-windows"
                type="number"
                min={2}
                max={20}
                step={1}
                value={nWindows}
                onChange={(e) => setNWindows(Number(e.target.value))}
              />
            </Field>
            <Field label="Train fraction" htmlFor="wf-trainfrac" hint="Fraction of each fold used for training (0–1)">
              <Input
                id="wf-trainfrac"
                type="number"
                min={0.1}
                max={0.95}
                step={0.05}
                value={trainFrac}
                onChange={(e) => setTrainFrac(Number(e.target.value))}
              />
            </Field>
          </div>

          <Field label="Fill timing" htmlFor="wf-fill-timing" hint="Next open = realistic; Close = legacy">
            <Select
              id="wf-fill-timing"
              value={fillTiming}
              onChange={(e) => setFillTiming(e.target.value as FillTiming)}
            >
              <option value="next_open">Next open (realistic)</option>
              <option value="close">Close (legacy)</option>
            </Select>
          </Field>
        </div>
      </Panel>

      {selectedStrategy && (
        <Panel title="Parameter ranges">
          <p className="mb-3 text-xs text-slate-500">
            Define ranges for the parameter grid. Best combo per fold is chosen by the selected metric.
          </p>
          <ParamRangeGrid
            params={selectedStrategy.params}
            ranges={paramRanges}
            onChange={setRange}
            idPrefix="wf-"
          />
        </Panel>
      )}

      <Button
        variant="teal"
        onClick={handleSubmit}
        disabled={!canRun}
        loading={running}
        className="w-full rounded-lg py-2.5 font-semibold"
      >
        {running ? "Running walk-forward…" : "Run walk-forward"}
      </Button>

      {!engineAvailable && (
        <p className="text-center text-xs text-amber-400">
          Engine unavailable — run <code>maturin develop</code> in backtesting-py.
        </p>
      )}
    </div>
  );
}
