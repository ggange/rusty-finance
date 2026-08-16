import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { Panel } from "../ui/Panel";
import { Spinner } from "../ui/Spinner";
import {
  AXIS_TICK,
  CHART_MARGIN,
  GRID,
  GRID_DASH,
  SERIES,
  TOOLTIP_STYLE,
} from "../../lib/chartTheme";
import { formatInterval, formatMetric, getMetricInterval, getMetricValue } from "../../lib/metrics";
import type { Metrics, WalkForwardFold } from "../../types/api";
import type { WalkForwardStatus } from "../../hooks/useWalkForward";

// ─── Per-fold table ──────────────────────────────────────────────────────────

function FoldTable({
  folds,
  metric,
}: {
  folds: WalkForwardFold[];
  metric: string;
}) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-slate-700 text-left text-xs text-slate-400">
            <th className="pb-2 pr-4 font-medium">Fold</th>
            <th className="pb-2 pr-4 font-medium">Train period</th>
            <th className="pb-2 pr-4 font-medium">Test period</th>
            <th className="pb-2 pr-4 font-medium">Best params</th>
            <th className="pb-2 pr-4 font-medium text-sky-400">Train {metric}</th>
            <th className="pb-2 font-medium text-amber-400">Test {metric}</th>
          </tr>
        </thead>
        <tbody>
          {folds.map((fold) => {
            const trainVal = getMetricValue(fold.train_metrics, metric);
            const testVal  = getMetricValue(fold.test_metrics, metric);
            const overfit  = trainVal > 0 && testVal < trainVal * 0.5; // simple overfit signal
            return (
              <tr key={fold.window_index} className="border-b border-slate-800">
                <td className="py-2 pr-4 text-slate-300">{fold.window_index + 1}</td>
                <td className="py-2 pr-4 font-mono text-xs text-slate-400">
                  {fold.train_range.start} → {fold.train_range.end}
                </td>
                <td className="py-2 pr-4 font-mono text-xs text-slate-400">
                  {fold.test_range.start} → {fold.test_range.end}
                </td>
                <td className="py-2 pr-4 text-xs text-slate-300">
                  {Object.entries(fold.best_params)
                    .map(([k, v]) => `${k}=${v}`)
                    .join(", ")}
                  {fold.tied_candidates > 1 && (
                    <span
                      className="ml-1 text-xs text-amber-500"
                      title={`${fold.tied_candidates} parameter sets tied on the train metric — these were not really selected, and comparing them across folds is reading noise`}
                    >
                      ({fold.tied_candidates}-way tie)
                    </span>
                  )}
                  {fold.tied_candidates === 0 && (
                    <span
                      className="ml-1 text-xs text-rose-500"
                      title="every combo scored NaN on the train slice; nothing was selectable"
                    >
                      (no valid candidate)
                    </span>
                  )}
                </td>
                <td className="py-2 pr-4 font-mono text-sky-300">
                  {formatMetric(trainVal, metric)}
                </td>
                <td className={`py-2 font-mono ${overfit ? "text-rose-400" : "text-amber-300"}`}>
                  {formatMetric(testVal, metric)}
                  {overfit && <span className="ml-1 text-xs text-rose-500" title="test significantly below train">⚠</span>}
                  {(() => {
                    const iv = getMetricInterval(fold.test_metrics, metric);
                    const u = fold.test_metrics.uncertainty;
                    if (!iv) return null;
                    return (
                      <span
                        className="ml-2 text-xs font-normal text-slate-500"
                        title={`${u?.observations ?? "?"} out-of-sample returns. A window this short cannot distinguish much — read the sign and the parameter stability, not the level.`}
                      >
                        {formatInterval(iv, metric, u?.confidence)}
                      </span>
                    );
                  })()}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

// ─── Train vs test bar chart ─────────────────────────────────────────────────

function TrainTestChart({
  folds,
  metric,
}: {
  folds: WalkForwardFold[];
  metric: string;
}) {
  const data = folds.map((fold) => ({
    fold: `Fold ${fold.window_index + 1}`,
    train: getMetricValue(fold.train_metrics, metric),
    test:  getMetricValue(fold.test_metrics, metric),
  }));

  const fmt = (v: number) => formatMetric(v, metric);

  return (
    <div className="h-64 w-full">
      <ResponsiveContainer>
        <BarChart data={data} margin={CHART_MARGIN}>
          <CartesianGrid strokeDasharray={GRID_DASH} stroke={GRID} />
          <XAxis dataKey="fold" tick={AXIS_TICK} />
          <YAxis tick={AXIS_TICK} width={64} tickFormatter={fmt} />
          <Tooltip
            contentStyle={TOOLTIP_STYLE}
            formatter={(v: number, name: string) => [fmt(v), name === "train" ? "Train" : "Test"]}
          />
          <Legend wrapperStyle={{ fontSize: 12, color: AXIS_TICK.fill }} />
          <Bar dataKey="train" name="Train" fill={SERIES.primary} radius={[3, 3, 0, 0]} />
          <Bar dataKey="test" name="Test" fill={SERIES.external} radius={[3, 3, 0, 0]} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

// ─── Main panel ──────────────────────────────────────────────────────────────

interface WalkForwardResultsPanelProps {
  folds: WalkForwardFold[];
  oos: Metrics | null;
  status: WalkForwardStatus;
  error: string | null;
  metric: string;
}

/** Pooled out-of-sample summary: the one number that describes the procedure. */
function PooledSummary({ oos, metric }: { oos: Metrics; metric: string }) {
  const iv = getMetricInterval(oos, metric);
  const u = oos.uncertainty;
  const value = getMetricValue(oos, metric);
  const straddlesZero = iv ? iv.lo < 0 && iv.hi > 0 : false;

  return (
    <div className="space-y-2">
      <p className="text-xs text-slate-500">
        Every fold's out-of-sample returns stitched into one series. This is
        <em> not</em> the average of the folds above — averaging treats each fold as
        an independent observation, while pooling treats the sequence as the single
        return path it actually is.
      </p>
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
        <span className="text-xs uppercase tracking-wide text-slate-500">{metric}</span>
        <span className="text-2xl font-semibold tabular-nums text-amber-300">
          {formatMetric(value, metric)}
        </span>
        {iv && (
          <span className="font-mono text-sm tabular-nums text-slate-400">
            {formatInterval(iv, metric, u?.confidence)}
          </span>
        )}
        {straddlesZero && (
          <span
            className="rounded bg-amber-950/60 px-2 py-0.5 text-xs text-amber-400"
            title="The interval contains zero, so this result is not distinguishable from no edge at this sample size."
          >
            not distinguishable from zero
          </span>
        )}
      </div>
      {u && (
        <p className="text-xs text-slate-600">
          {u.observations} out-of-sample returns · {u.resamples} resamples · mean block{" "}
          {u.mean_block.toFixed(1)} bars. Sampling error only: this does not correct for
          choosing parameters by arg-max over the grid.
        </p>
      )}
    </div>
  );
}

export function WalkForwardResultsPanel({
  folds,
  oos,
  status,
  error,
  metric,
}: WalkForwardResultsPanelProps) {
  if (status === "idle") {
    return (
      <Panel>
        <p className="py-16 text-center text-slate-500">
          Configure a dataset, strategy, and parameter ranges, then run walk-forward validation.
        </p>
      </Panel>
    );
  }

  if (status === "loading") {
    return (
      <Panel>
        <div className="flex justify-center py-16">
          <Spinner label="Running walk-forward validation…" />
        </div>
      </Panel>
    );
  }

  if (status === "error") {
    return (
      <Panel title="Error">
        <p className="rounded-md bg-rose-950/60 px-3 py-2 text-sm text-rose-300">{error}</p>
      </Panel>
    );
  }

  if (folds.length === 0) {
    return (
      <Panel title="Walk-forward results">
        <p className="py-8 text-center text-slate-500">No folds returned.</p>
      </Panel>
    );
  }

  return (
    <div className="space-y-4">
      <Panel title={`Walk-forward — ${folds.length} fold${folds.length !== 1 ? "s" : ""}`}>
        <p className="mb-3 text-xs text-slate-500">
          Train (blue) vs test (orange) {metric}. Test ≪ Train = overfit signal (⚠).
        </p>
        <TrainTestChart folds={folds} metric={metric} />
      </Panel>

      {oos && (
        <Panel title="Pooled out-of-sample">
          <PooledSummary oos={oos} metric={metric} />
        </Panel>
      )}

      <Panel title="Per-fold breakdown">
        <FoldTable folds={folds} metric={metric} />
      </Panel>
    </div>
  );
}
