// Shared recharts theming. These values were previously copy-pasted into every
// chart component; changing a colour meant editing six files. Import from here
// instead so the charts stay consistent with each other and with the Tailwind
// slate palette used by the surrounding UI.

/** slate-700 — cartesian grid lines. */
export const GRID = "#334155";
/** slate-400 — axis tick labels. */
export const AXIS_TICK_FILL = "#94a3b8";
/** slate-900 — tooltip background. */
export const SURFACE = "#0f172a";

export const SERIES = {
  /** sky-400 — the primary series (strategy / portfolio NAV). */
  primary: "#38bdf8",
  /** neutral-400 — buy-and-hold reference. */
  benchmark: "#a3a3a3",
  /** orange-400 — external benchmark symbol. */
  external: "#fb923c",
  /** violet-600 — rebalance markers. */
  marker: "#7c3aed",
  /** rose-500 — drawdown. */
  negative: "#f43f5e",
  /** indigo-500 — rolling volatility. */
  volatility: "#6366f1",
  /** emerald-400 — out-of-sample / positive series. */
  positive: "#34d399",
} as const;

/** Standard axis tick styling. Spread into a recharts `tick` prop. */
export const AXIS_TICK = { fontSize: 11, fill: AXIS_TICK_FILL } as const;

/** Standard tooltip `contentStyle`. */
export const TOOLTIP_STYLE = {
  background: SURFACE,
  border: `1px solid ${GRID}`,
  borderRadius: 8,
  fontSize: 12,
} as const;

export const LEGEND_STYLE = { fontSize: 12 } as const;

/** Every chart uses the same plot margins. */
export const CHART_MARGIN = { top: 8, right: 16, bottom: 8, left: 8 } as const;

export const GRID_DASH = "3 3";
