import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SelectionPanel } from "./SelectionPanel";
import { SweepResultsPanel } from "./SweepResultsPanel";
import type { Metrics, SelectionCorrection, SweepPoint } from "../../types/api";

// The real MSFT / RSI 5..30 result: Sharpe 0.55 at period 7, but SR* is 0.34 and
// the deflated Sharpe lands at 0.708 — below the 0.95 bar.
const correction: SelectionCorrection = {
  method: "deflated_sharpe_ratio",
  trials: 26,
  trials_run: 26,
  trials_that_traded: 22,
  trials_overridden: false,
  best_index: 2,
  tied_at_best: 1,
  observations: 1652,
  sharpe_ratio: 0.550717,
  trial_sharpe_std_dev: 0.168015,
  expected_max_sharpe: 0.338338,
  skewness: 0.75079,
  kurtosis: 24.328989,
  probabilistic_sharpe: 0.922664,
  deflated_sharpe: 0.708446,
  degenerate_trials_dominate: false,
};

const metrics: Metrics = {
  total_return: 0.42,
  cagr: 0.09,
  annualized_volatility: 0.21,
  max_drawdown: -0.19,
  sharpe_ratio: 0.550717,
  sortino_ratio: 0.7,
  win_rate: 0.55,
  trade_count: 40,
};

const grid: SweepPoint[] = [5, 6, 7].map((period, i) => ({
  params: { period },
  metrics: { ...metrics, sharpe_ratio: i === 2 ? 0.550717 : 0.2 },
}));

describe("SelectionPanel", () => {
  it("reports the deflated Sharpe beside the uncorrected one", () => {
    render(<SelectionPanel selection={correction} metric="sharpe_ratio" />);
    // The gap between the two is the cost of the search, so both have to be on
    // screen — showing only the deflated figure hides how much was deflated.
    expect(screen.getByText("70.8%")).toBeInTheDocument();
    expect(screen.getByText(/uncorrected 92\.3%/)).toBeInTheDocument();
  });

  it("says so when the winner does not survive the correction", () => {
    render(<SelectionPanel selection={correction} metric="sharpe_ratio" />);
    expect(screen.getByText(/not significant once the search is counted/)).toBeInTheDocument();
  });

  it("drops the warning once the winner clears the bar", () => {
    render(
      <SelectionPanel
        selection={{ ...correction, deflated_sharpe: 0.97 }}
        metric="sharpe_ratio"
      />,
    );
    expect(screen.queryByText(/not significant/)).not.toBeInTheDocument();
    expect(screen.getByText("97.0%")).toBeInTheDocument();
  });

  it("flags a tie at the top, because a tied search selected nothing", () => {
    render(
      <SelectionPanel selection={{ ...correction, tied_at_best: 4 }} metric="sharpe_ratio" />,
    );
    expect(screen.getByText("4-way tie at the top")).toBeInTheDocument();
  });

  it("flags a grid whose Sharpe spread is an artefact of idle cells", () => {
    render(
      <SelectionPanel
        selection={{ ...correction, trials_that_traded: 3, degenerate_trials_dominate: true }}
        metric="sharpe_ratio"
      />,
    );
    expect(screen.getByText("only 3 of 26 cells traded")).toBeInTheDocument();
  });

  it("warns that the correction describes the Sharpe-selected cell, not another metric's", () => {
    // The deflation's distributional assumptions are Sharpe's. When the chart
    // ranks by CAGR the two arg-maxes can differ, and reading this number as the
    // CAGR winner's would be wrong.
    render(<SelectionPanel selection={correction} metric="cagr" />);
    expect(screen.getByText(/applies to the Sharpe-selected cell/)).toBeInTheDocument();
  });

  it("stays silent about the metric when the sweep is ranked by Sharpe", () => {
    render(<SelectionPanel selection={correction} metric="sharpe_ratio" />);
    expect(screen.queryByText(/applies to the Sharpe-selected cell/)).not.toBeInTheDocument();
  });

  it("shows where the trial count came from when it was overridden", () => {
    render(
      <SelectionPanel
        selection={{ ...correction, trials: 5000, trials_overridden: true }}
        metric="sharpe_ratio"
      />,
    );
    expect(screen.getByText(/5000 trials \(overridden from 26\)/)).toBeInTheDocument();
  });
});

describe("SweepResultsPanel", () => {
  it("renders the correction under the best-combination callout", () => {
    render(
      <SweepResultsPanel
        results={grid}
        selection={correction}
        status="success"
        error={null}
        metric="sharpe_ratio"
      />,
    );
    expect(screen.getByText("Best combination")).toBeInTheDocument();
    expect(screen.getByText("Deflated Sharpe ratio")).toBeInTheDocument();
  });

  it("renders exactly as before when no correction was computed", () => {
    // A single-combination grid, a disabled config, or a degenerate winner all
    // arrive as a null selection, and none of them is a reason to change the grid.
    render(
      <SweepResultsPanel
        results={grid}
        selection={null}
        status="success"
        error={null}
        metric="sharpe_ratio"
      />,
    );
    expect(screen.getByText("Best combination")).toBeInTheDocument();
    expect(screen.queryByText("Deflated Sharpe ratio")).not.toBeInTheDocument();
  });
});
