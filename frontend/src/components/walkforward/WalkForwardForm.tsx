import { useState, useEffect } from "react";
import { Field } from "../ui/Field";
import { Panel } from "../ui/Panel";
import type {
  Dataset,
  FillTiming,
  ParamRange,
  StrategyMeta,
  StrategyType,
  WalkForwardRequest,
} from "../../types/api";

const METRIC_OPTIONS = [
  { value: "sharpe_ratio",          label: "Sharpe ratio" },
  { value: "total_return",          label: "Total return" },
  { value: "cagr",                  label: "CAGR" },
  { value: "sortino_ratio",         label: "Sortino ratio" },
  { value: "max_drawdown",          label: "Max drawdown (lower is better)" },
  { value: "annualized_volatility", label: "Ann. volatility (lower is better)" },
];

const selectClass =
  "w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none";
const inputClass =
  "w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none";

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
            <select
              id="wf-dataset"
              value={dataset}
              onChange={(e) => setDataset(e.target.value)}
              className={selectClass}
            >
              {datasets.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.symbol}
                </option>
              ))}
            </select>
          </Field>

          <Field label="Strategy" htmlFor="wf-strategy">
            <select
              id="wf-strategy"
              value={strategyType}
              onChange={(e) => setStrategyType(e.target.value as StrategyType)}
              className={selectClass}
            >
              {strategies.map((s) => (
                <option key={s.type} value={s.type}>
                  {s.name}
                </option>
              ))}
            </select>
          </Field>

          <Field label="Optimise by" htmlFor="wf-metric">
            <select
              id="wf-metric"
              value={metric}
              onChange={(e) => setMetric(e.target.value)}
              className={selectClass}
            >
              {METRIC_OPTIONS.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </select>
          </Field>

          <div className="grid grid-cols-2 gap-3">
            <Field label="Windows" htmlFor="wf-windows" hint="Number of rolling folds (≥ 2)">
              <input
                id="wf-windows"
                type="number"
                min={2}
                max={20}
                step={1}
                value={nWindows}
                onChange={(e) => setNWindows(Number(e.target.value))}
                className={inputClass}
              />
            </Field>
            <Field label="Train fraction" htmlFor="wf-trainfrac" hint="Fraction of each fold used for training (0–1)">
              <input
                id="wf-trainfrac"
                type="number"
                min={0.1}
                max={0.95}
                step={0.05}
                value={trainFrac}
                onChange={(e) => setTrainFrac(Number(e.target.value))}
                className={inputClass}
              />
            </Field>
          </div>

          <Field label="Fill timing" htmlFor="wf-fill-timing" hint="Next open = realistic; Close = legacy">
            <select
              id="wf-fill-timing"
              value={fillTiming}
              onChange={(e) => setFillTiming(e.target.value as FillTiming)}
              className={selectClass}
            >
              <option value="next_open">Next open (realistic)</option>
              <option value="close">Close (legacy)</option>
            </select>
          </Field>
        </div>
      </Panel>

      {selectedStrategy && (
        <Panel title="Parameter ranges">
          <p className="mb-3 text-xs text-slate-500">
            Define ranges for the parameter grid. Best combo per fold is chosen by the selected metric.
          </p>
          <div className="space-y-4">
            {selectedStrategy.params.map((p) => {
              const rng = paramRanges[p.name] ?? { min: p.default, max: p.default, step: 1 };
              return (
                <div key={p.name}>
                  <p className="mb-1.5 text-sm font-medium text-slate-300">{p.name}</p>
                  <div className="grid grid-cols-3 gap-2">
                    <Field label="Min" htmlFor={`wf-${p.name}-min`}>
                      <input
                        id={`wf-${p.name}-min`}
                        type="number"
                        min={p.min}
                        step={p.step ?? 1}
                        value={rng.min}
                        onChange={(e) => setRange(p.name, "min", Number(e.target.value))}
                        className={inputClass}
                      />
                    </Field>
                    <Field label="Max" htmlFor={`wf-${p.name}-max`}>
                      <input
                        id={`wf-${p.name}-max`}
                        type="number"
                        min={p.min}
                        step={p.step ?? 1}
                        value={rng.max}
                        onChange={(e) => setRange(p.name, "max", Number(e.target.value))}
                        className={inputClass}
                      />
                    </Field>
                    <Field label="Step" htmlFor={`wf-${p.name}-step`}>
                      <input
                        id={`wf-${p.name}-step`}
                        type="number"
                        min={0.001}
                        step={p.step ?? 1}
                        value={rng.step}
                        onChange={(e) => setRange(p.name, "step", Number(e.target.value))}
                        className={inputClass}
                      />
                    </Field>
                  </div>
                </div>
              );
            })}
          </div>
        </Panel>
      )}

      <button
        type="button"
        onClick={handleSubmit}
        disabled={!canRun}
        className="w-full rounded-lg bg-teal-600 px-4 py-2.5 font-semibold text-white transition hover:bg-teal-500 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-500"
      >
        {running ? "Running walk-forward…" : "Run walk-forward"}
      </button>

      {!engineAvailable && (
        <p className="text-center text-xs text-amber-400">
          Engine unavailable — run <code>maturin develop</code> in backtesting-py.
        </p>
      )}
    </div>
  );
}
