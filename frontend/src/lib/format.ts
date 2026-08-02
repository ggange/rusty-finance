// Formatting helpers for metrics, currency, and dates.

export function formatPct(value: number | null, digits = 2): string {
  if (value === null || value === undefined || Number.isNaN(value)) return "—";
  return `${(value * 100).toFixed(digits)}%`;
}

export function formatNum(value: number | null, digits = 2): string {
  if (value === null || value === undefined || Number.isNaN(value)) return "—";
  return value.toFixed(digits);
}

export function formatCurrency(value: number | null, digits = 2): string {
  if (value === null || value === undefined || Number.isNaN(value)) return "—";
  return value.toLocaleString(undefined, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
}

export function formatInt(value: number): string {
  return value.toLocaleString();
}

/**
 * Share quantities. The trading loop sizes positions by cash, so quantities are
 * routinely fractional (32.37188786847498 shares) — rounding those to whole
 * numbers would misstate the position. Whole numbers still print cleanly.
 */
export function formatQty(value: number | null, maxDigits = 4): string {
  if (value === null || value === undefined || Number.isNaN(value)) return "—";
  return value.toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: Number.isInteger(value) ? 0 : maxDigits,
  });
}

/**
 * The ledger stores a strategy as the serialized params blob
 * (`{"period": 5, "type": "rsi"}`), which is unreadable in a table cell.
 * Render it as `rsi(period=5)`, falling back to the raw string if it isn't JSON.
 */
export function formatStrategy(raw: string): string {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return raw;
  }
  if (typeof parsed !== "object" || parsed === null) return raw;

  const { type, ...params } = parsed as Record<string, unknown>;
  const args = Object.entries(params)
    .map(([k, v]) => `${k}=${v}`)
    .join(", ");
  const name = typeof type === "string" ? type : "strategy";
  return args ? `${name}(${args})` : name;
}

// Tailwind text color class based on sign (positive green, negative red).
export function signClass(value: number | null): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return "text-slate-300";
  }
  if (value > 0) return "text-emerald-400";
  if (value < 0) return "text-rose-400";
  return "text-slate-300";
}
