import { useState, useEffect } from "react";
import { Field } from "../ui/Field";
import { Panel } from "../ui/Panel";
import type { Dataset, ParamRange, StrategyMeta, StrategyType, SweepRequest } from "../../types/api";

const METRIC_OPTIONS = [
  { value: "sharpe_ratio", label: "Sharpe ratio" },
  { value: "total_return", label: "Total return" },
  { value: "cagr", label: "CAGR" },
  { value: "sortino_ratio", label: "Sortino ratio" },
  { value: "max_drawdown", label: "Max drawdown (lower is better)" },
];

const selectClass =
  "w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none";
const inputClass =
  "w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 focus:border-sky-400 focus:outline-none";

interface SweepFormProps {
  strategies: StrategyMeta[];
  datasets: Dataset[];
  initialCash: number;
  commission: number;
  slippagePct: number;
  onRun: (req: SweepRequest, metric: string) => void;
  running: boolean;
  engineAvailable: boolean;
}

export function SweepForm({
  strategies,
  datasets,
  initialCash,
  commission,
  slippagePct,
  onRun,
  running,
  engineAvailable,
}: SweepFormProps) {
  const [dataset, setDataset] = useState(datasets[0]?.name ?? "");
  const [strategyType, setStrategyType] = useState<StrategyType>(strategies[0]?.type ?? "ma_ema");
  const [metric, setMetric] = useState("sharpe_ratio");
  const [paramRanges, setParamRanges] = useState<Record<string, ParamRange>>({});

  const selectedStrategy = strategies.find((s) => s.type === strategyType);

  // Re-seed param ranges when strategy changes
  useEffect(() => {
    if (!selectedStrategy) return;
    const ranges: Record<string, ParamRange> = {};
    for (const p of selectedStrategy.params) {
      // Use a sensible default range per param type
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

  const variedCount = Object.values(paramRanges).filter((r) => r.min !== r.max).length;

  function handleSubmit() {
    if (!dataset || !strategyType) return;
    onRun(
      {
        dataset,
        strategy_type: strategyType,
        param_ranges: paramRanges,
        initial_cash: initialCash,
        commission,
        slippage_pct: slippagePct,
      },
      metric,
    );
  }

  const canRun = !!dataset && !!strategyType && engineAvailable && !running;

  return (
    <div className="space-y-4">
      <Panel title="Sweep settings">
        <div className="space-y-3">
          <Field label="Dataset" htmlFor="sweep-dataset">
            <select
              id="sweep-dataset"
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

          <Field label="Strategy" htmlFor="sweep-strategy">
            <select
              id="sweep-strategy"
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

          <Field label="Metric" htmlFor="sweep-metric">
            <select
              id="sweep-metric"
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
        </div>
      </Panel>

      {selectedStrategy && (
        <Panel title="Parameter ranges">
          <p className="mb-3 text-xs text-slate-500">
            Set min = max to fix a parameter. Vary 1 param for a bar chart, 2 for a heatmap.
          </p>
          <div className="space-y-4">
            {selectedStrategy.params.map((p) => {
              const rng = paramRanges[p.name] ?? { min: p.default, max: p.default, step: 1 };
              return (
                <div key={p.name}>
                  <p className="mb-1.5 text-sm font-medium text-slate-300">{p.name}</p>
                  <div className="grid grid-cols-3 gap-2">
                    <Field label="Min" htmlFor={`${p.name}-min`}>
                      <input
                        id={`${p.name}-min`}
                        type="number"
                        min={p.min}
                        step={p.step ?? 1}
                        value={rng.min}
                        onChange={(e) => setRange(p.name, "min", Number(e.target.value))}
                        className={inputClass}
                      />
                    </Field>
                    <Field label="Max" htmlFor={`${p.name}-max`}>
                      <input
                        id={`${p.name}-max`}
                        type="number"
                        min={p.min}
                        step={p.step ?? 1}
                        value={rng.max}
                        onChange={(e) => setRange(p.name, "max", Number(e.target.value))}
                        className={inputClass}
                      />
                    </Field>
                    <Field label="Step" htmlFor={`${p.name}-step`}>
                      <input
                        id={`${p.name}-step`}
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
          {variedCount > 2 && (
            <p className="mt-3 text-xs text-amber-400">
              Only 1 or 2 parameters can vary for a chart — set extras to min = max.
            </p>
          )}
        </Panel>
      )}

      <button
        type="button"
        onClick={handleSubmit}
        disabled={!canRun || variedCount > 2}
        className="w-full rounded-lg bg-violet-600 px-4 py-2.5 font-semibold text-white transition hover:bg-violet-500 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-500"
      >
        {running ? "Running sweep…" : "Run sweep"}
      </button>

      {!engineAvailable && (
        <p className="text-center text-xs text-amber-400">
          Engine unavailable — run <code>maturin develop</code> in backtesting-py.
        </p>
      )}
    </div>
  );
}
