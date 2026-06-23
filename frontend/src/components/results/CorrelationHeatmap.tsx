interface CorrelationHeatmapProps {
  symbols: string[];
  correlation: number[][];
}

function cellColor(v: number): string {
  if (v >= 0) {
    // 0 → slate, 1 → emerald
    const t = Math.min(v, 1);
    const r = Math.round(30 + (16 - 30) * t);
    const g = Math.round(41 + (185 - 41) * t);
    const b = Math.round(59 + (129 - 59) * t);
    return `rgb(${r},${g},${b})`;
  } else {
    // -1 → rose, 0 → slate
    const t = Math.min(-v, 1);
    const r = Math.round(30 + (244 - 30) * t);
    const g = Math.round(41 + (63 - 41) * t);
    const b = Math.round(59 + (94 - 59) * t);
    return `rgb(${r},${g},${b})`;
  }
}

export function CorrelationHeatmap({ symbols, correlation }: CorrelationHeatmapProps) {
  const n = symbols.length;
  if (n < 2) {
    return (
      <p className="text-xs text-slate-500">
        Correlation heatmap requires at least 2 assets.
      </p>
    );
  }

  return (
    <div className="overflow-auto">
      <table className="border-separate border-spacing-0.5 text-xs">
        <thead>
          <tr>
            <th className="w-14" />
            {symbols.map((s) => (
              <th key={s} className="px-2 py-1 text-center font-medium text-slate-400">
                {s}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {symbols.map((rowSym, i) => (
            <tr key={rowSym}>
              <td className="pr-2 text-right font-medium text-slate-400">{rowSym}</td>
              {symbols.map((_, j) => {
                const v = correlation[i]?.[j] ?? 0;
                return (
                  <td
                    key={j}
                    title={`${symbols[i]} / ${symbols[j]}: ${v.toFixed(3)}`}
                    style={{ backgroundColor: cellColor(v) }}
                    className="h-8 w-12 rounded text-center tabular-nums text-slate-100"
                  >
                    {v.toFixed(2)}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
