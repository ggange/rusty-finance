import { useState, useEffect } from "react";
import { ParamRangeGrid } from "../config/ParamRangeGrid";
import { Button } from "../ui/Button";
import { Field } from "../ui/Field";
import { Panel } from "../ui/Panel";
import { Select } from "../ui/Select";
import { METRIC_OPTIONS } from "../../lib/metrics";
import type { Dataset, ParamRange, StrategyMeta, StrategyType, SweepRequest } from "../../types/api";

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
            <Select
              id="sweep-dataset"
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

          <Field label="Strategy" htmlFor="sweep-strategy">
            <Select
              id="sweep-strategy"
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

          <Field label="Metric" htmlFor="sweep-metric">
            <Select
              id="sweep-metric"
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
        </div>
      </Panel>

      {selectedStrategy && (
        <Panel title="Parameter ranges">
          <p className="mb-3 text-xs text-slate-500">
            Set min = max to fix a parameter. Vary 1 param for a bar chart, 2 for a heatmap.
          </p>
          <ParamRangeGrid
            params={selectedStrategy.params}
            ranges={paramRanges}
            onChange={setRange}
            idPrefix="sweep-"
          />
          {variedCount > 2 && (
            <p className="mt-3 text-xs text-amber-400">
              Only 1 or 2 parameters can vary for a chart — set extras to min = max.
            </p>
          )}
        </Panel>
      )}

      <Button
        variant="violet"
        onClick={handleSubmit}
        disabled={!canRun || variedCount > 2}
        loading={running}
        className="w-full rounded-lg py-2.5 font-semibold"
      >
        {running ? "Running sweep…" : "Run sweep"}
      </Button>

      {!engineAvailable && (
        <p className="text-center text-xs text-amber-400">
          Engine unavailable — run <code>maturin develop</code> in backtesting-py.
        </p>
      )}
    </div>
  );
}
