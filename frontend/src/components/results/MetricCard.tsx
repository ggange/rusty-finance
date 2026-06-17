interface MetricCardProps {
  label: string;
  value: string;
  valueClass?: string;
  benchmark?: string; // optional "vs bench" line
}

export function MetricCard({
  label,
  value,
  valueClass = "text-slate-100",
  benchmark,
}: MetricCardProps) {
  return (
    <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-3">
      <p className="text-xs uppercase tracking-wide text-slate-500">{label}</p>
      <p className={`mt-1 text-xl font-semibold tabular-nums ${valueClass}`}>
        {value}
      </p>
      {benchmark && (
        <p className="mt-0.5 text-xs text-slate-500">{benchmark}</p>
      )}
    </div>
  );
}
