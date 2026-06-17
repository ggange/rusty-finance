import type { EquityPoint } from "../types/api";

export interface DrawdownPoint {
  date: string;
  drawdown: number; // fraction <= 0, e.g. -0.12 means 12% below the running peak
}

/**
 * Derive the underwater (drawdown) series from an equity curve.
 * drawdown[i] = (nav[i] - runningPeak) / runningPeak, always <= 0.
 * The minimum of this series should match metrics.max_drawdown.
 */
export function deriveDrawdown(curve: EquityPoint[]): DrawdownPoint[] {
  let peak = -Infinity;
  return curve.map(({ date, nav }) => {
    peak = Math.max(peak, nav);
    return { date, drawdown: peak > 0 ? nav / peak - 1 : 0 };
  });
}
