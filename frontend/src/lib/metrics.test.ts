import { describe, expect, it } from "vitest";
import {
  formatDrawdownSpread,
  formatInterval,
  getMetricInterval,
  getMetricValue,
} from "./metrics";
import type { Metrics, MetricUncertainty } from "../types/api";

const bare: Metrics = {
  total_return: 0.42,
  cagr: 0.18,
  annualized_volatility: 0.21,
  max_drawdown: -0.13,
  sharpe_ratio: 1.18,
  sortino_ratio: 1.64,
  win_rate: 0.57,
  trade_count: 12,
};

const uncertainty: MetricUncertainty = {
  method: "stationary_bootstrap",
  confidence: 0.95,
  resamples: 1000,
  mean_block: 7.9,
  seed: 42,
  observations: 495,
  sharpe_ratio: { lo: -1.135, hi: 1.519, std_error: 0.685 },
  sortino_ratio: { lo: -1.4, hi: 2.0, std_error: 0.9 },
  cagr: { lo: -0.11, hi: 0.24, std_error: 0.09 },
  max_drawdown_std_error: 0.1056,
};

const bounded: Metrics = { ...bare, uncertainty };

describe("getMetricInterval", () => {
  it("reads the interval for a metric that has one", () => {
    expect(getMetricInterval(bounded, "sharpe_ratio")).toEqual(uncertainty.sharpe_ratio);
  });

  it("returns null when the response carried no uncertainty", () => {
    expect(getMetricInterval(bare, "sharpe_ratio")).toBeNull();
  });

  it("returns null for metrics the bootstrap does not bound", () => {
    // Max drawdown gets a standard error but no endpoints, and total return and
    // volatility are not bounded at all.
    expect(getMetricInterval(bounded, "max_drawdown")).toBeNull();
    expect(getMetricInterval(bounded, "total_return")).toBeNull();
    expect(getMetricInterval(bounded, "annualized_volatility")).toBeNull();
  });

  it("leaves getMetricValue returning plain numbers", () => {
    // The reason the interval is a sibling object rather than replacing the
    // metric: these values feed chart scales directly, and an object here would
    // break the charts with no error.
    expect(getMetricValue(bounded, "sharpe_ratio")).toBe(1.18);
    expect(typeof getMetricValue(bounded, "cagr")).toBe("number");
  });
});

describe("formatInterval", () => {
  it("formats a ratio metric as a plain range", () => {
    expect(formatInterval(uncertainty.sharpe_ratio, "sharpe_ratio", 0.95)).toBe(
      "95% CI -1.14 … 1.52",
    );
  });

  it("formats a percentage metric as percentages", () => {
    expect(formatInterval(uncertainty.cagr, "cagr", 0.95)).toBe("95% CI -11.00% … 24.00%");
  });

  it("labels the confidence from the response, not a hardcoded 95", () => {
    expect(formatInterval(uncertainty.sharpe_ratio, "sharpe_ratio", 0.99)).toContain("99% CI");
  });

  it("returns undefined when there is no interval, so it drops into an optional prop", () => {
    expect(formatInterval(null, "sharpe_ratio")).toBeUndefined();
  });
});

describe("formatDrawdownSpread", () => {
  it("renders a one-sigma spread in percentage points", () => {
    expect(formatDrawdownSpread(bounded)).toBe("± 10.6pp (1 s.e.)");
  });

  it("returns undefined when unbounded", () => {
    expect(formatDrawdownSpread(bare)).toBeUndefined();
  });
});
