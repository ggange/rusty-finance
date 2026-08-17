import type { SelectionCorrection } from "../../types/api";

/**
 * The correction on the "best combination" callout above it.
 *
 * Deliberately cool-toned rather than emerald: this is not a second success
 * badge, it is the sweep reporting how much of its own headline is search.
 */
export function SelectionPanel({
  selection,
  metric,
}: {
  selection: SelectionCorrection;
  metric: string;
}) {
  const {
    trials,
    trials_run,
    trials_that_traded,
    trials_overridden,
    tied_at_best,
    observations,
    sharpe_ratio,
    trial_sharpe_std_dev,
    expected_max_sharpe,
    skewness,
    kurtosis,
    probabilistic_sharpe,
    deflated_sharpe,
    degenerate_trials_dominate,
  } = selection;

  const significant = deflated_sharpe >= 0.95;
  const pct = (p: number) => `${(p * 100).toFixed(1)}%`;

  return (
    <div className="rounded-md border border-slate-700 bg-slate-900/60 px-4 py-3 text-sm">
      <div className="mb-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <p className="font-medium text-slate-300">Deflated Sharpe ratio</p>
        <span
          className={`font-mono text-lg font-semibold tabular-nums ${
            significant ? "text-emerald-400" : "text-amber-400"
          }`}
        >
          {pct(deflated_sharpe)}
        </span>
        <span
          className="text-xs text-slate-500"
          title="The same probability without the correction: P(true Sharpe > 0), ignoring that this cell was chosen as the maximum of the grid."
        >
          uncorrected {pct(probabilistic_sharpe)}
        </span>
      </div>

      <p className="mb-2 text-xs text-slate-500">
        The probability the best cell's Sharpe of {sharpe_ratio.toFixed(2)} beats what
        a search of {trials} trials reaches with <em>no skill at all</em> — which
        here is {expected_max_sharpe.toFixed(2)}. Searching harder raises that bar,
        so this is the number the green box above cannot report about itself.
      </p>

      <div className="flex flex-wrap gap-2">
        {!significant && (
          <span
            className="rounded bg-amber-950/60 px-2 py-0.5 text-xs text-amber-400"
            title="Below 95%, the winning cell is not distinguishable from the best of this many coin flips. That is a finding about the search, not a bug."
          >
            not significant once the search is counted
          </span>
        )}
        {tied_at_best > 1 && (
          <span
            className="rounded bg-amber-950/60 px-2 py-0.5 text-xs text-amber-400"
            title="Cells sharing the winning Sharpe exactly. The search did not really select this one; the first was kept."
          >
            {tied_at_best}-way tie at the top
          </span>
        )}
        {degenerate_trials_dominate && (
          <span
            className="rounded bg-amber-950/60 px-2 py-0.5 text-xs text-amber-400"
            title="Cells that never trade score exactly zero, so the Sharpe spread is measuring the gap to a block of idle cells rather than the shape of the search space. The correction still holds, but read it as a lower bound on the penalty."
          >
            only {trials_that_traded} of {trials_run} cells traded
          </span>
        )}
        {metric !== "sharpe_ratio" && (
          <span
            className="rounded bg-slate-800 px-2 py-0.5 text-xs text-slate-400"
            title={`The chart above ranks by ${metric}, but the deflation is defined for the Sharpe ratio and describes the Sharpe-selected cell. Applying this formula to a ${metric} arg-max would be wrong.`}
          >
            applies to the Sharpe-selected cell, not the {metric} one
          </span>
        )}
      </div>

      <p className="mt-2 text-xs text-slate-600">
        {trials} trials{trials_overridden ? ` (overridden from ${trials_run})` : ""} · Sharpe
        spread across trials {trial_sharpe_std_dev.toFixed(2)} · {observations} observations ·
        skew {skewness.toFixed(2)} · kurtosis {kurtosis.toFixed(1)}
      </p>
      <p className="mt-1 text-xs text-slate-600">
        Counts the trials in <em>this</em> grid only. It cannot see the other
        strategies, assets, or earlier grids you have run, so treat it as an upper
        bound on significance — raise the trial count in the form to deflate against
        the honest number.
      </p>
    </div>
  );
}
