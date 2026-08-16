interface MetricCardProps {
  label: string;
  value: string;
  valueClass?: string;
  benchmark?: string; // optional "vs bench" line
  /** Optional uncertainty line, e.g. "95% CI −2.41 … 4.83". */
  interval?: string;
  /** Hover text for the interval line. */
  intervalTitle?: string;
}

export function MetricCard({
  label,
  value,
  valueClass = "text-slate-100",
  benchmark,
  interval,
  intervalTitle,
}: MetricCardProps) {
  return (
    <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-3">
      <p className="text-xs uppercase tracking-wide text-slate-500">{label}</p>
      <p className={`mt-1 text-xl font-semibold tabular-nums ${valueClass}`}>
        {value}
      </p>
      {interval && (
        <p
          className="mt-0.5 text-xs tabular-nums text-slate-400"
          title={intervalTitle}
        >
          {interval}
        </p>
      )}
      {benchmark && (
        <p className="mt-0.5 text-xs text-slate-500">{benchmark}</p>
      )}
    </div>
  );
}
