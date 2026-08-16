import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PortfolioResultsPanel } from "./PortfolioResultsPanel";
import { TradeLogTable } from "./TradeLogTable";
import { MetricCards } from "./MetricCards";
import { candles, portfolioResult } from "../../test/portfolioFixture";
import type { Metrics, MetricUncertainty } from "../../types/api";

describe("PortfolioResultsPanel", () => {
  it("prompts for configuration before anything has run", () => {
    render(
      <PortfolioResultsPanel status="idle" error={null} result={null} ranAssets={[]} />,
    );
    expect(screen.getByText(/configure/i)).toBeInTheDocument();
  });

  it("surfaces the API error message on failure", () => {
    render(
      <PortfolioResultsPanel
        status="error"
        error="backtesting_py not installed"
        result={null}
        ranAssets={[]}
      />,
    );
    expect(screen.getByText("backtesting_py not installed")).toBeInTheDocument();
  });

  it("renders metrics and charts for a successful run", () => {
    render(
      <PortfolioResultsPanel
        status="success"
        error={null}
        result={portfolioResult}
        ranAssets={[{ symbol: "MSFT", candles }]}
      />,
    );

    // The fixture reuses one Metrics object for the portfolio and its single
    // asset, so these appear in both the top cards and the breakdown.
    expect(screen.getAllByText("42.00%").length).toBeGreaterThan(0); // total return
    expect(screen.getAllByText("1.18").length).toBeGreaterThan(0); // sharpe
    expect(screen.getAllByText("MSFT").length).toBeGreaterThan(0);
  });
});

describe("TradeLogTable", () => {
  it("explains an empty trade log rather than rendering an empty table", () => {
    render(<TradeLogTable trades={[]} />);
    expect(screen.getByText(/no trades were executed/i)).toBeInTheDocument();
  });

  it("shows a dash for the opening leg's undefined PnL", () => {
    render(<TradeLogTable trades={portfolioResult.assets[0].trades} />);
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.getByText("$1,468.00")).toBeInTheDocument();
  });

  it("distinguishes buys from sells", () => {
    render(<TradeLogTable trades={portfolioResult.assets[0].trades} />);
    expect(screen.getByText("Buy").className).toContain("emerald");
    expect(screen.getByText("Sell").className).toContain("rose");
  });
});

describe("MetricCards uncertainty", () => {
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
  const benchmark = { total_return: 0.3, cagr: 0.12 };

  it("renders nothing extra when the response carried no interval", () => {
    const bare: Metrics = { ...portfolioResult.metrics, uncertainty: undefined };
    render(<MetricCards metrics={bare} benchmark={benchmark} />);
    expect(screen.queryByText(/CI/)).not.toBeInTheDocument();
    expect(screen.queryByText(/s\.e\./)).not.toBeInTheDocument();
  });

  it("shows a confidence interval under Sharpe, Sortino and CAGR", () => {
    const bounded: Metrics = { ...portfolioResult.metrics, uncertainty };
    render(<MetricCards metrics={bounded} benchmark={benchmark} />);
    expect(screen.getByText("95% CI -1.14 … 1.52")).toBeInTheDocument();
    expect(screen.getByText("95% CI -1.40 … 2.00")).toBeInTheDocument();
    expect(screen.getByText("95% CI -11.00% … 24.00%")).toBeInTheDocument();
  });

  it("shows max drawdown as a spread, never as an interval", () => {
    // Percentile endpoints on a drawdown would be biased toward optimism, since
    // block resampling breaks up the trends that produce deep drawdowns.
    const bounded: Metrics = { ...portfolioResult.metrics, uncertainty };
    render(<MetricCards metrics={bounded} benchmark={benchmark} />);
    expect(screen.getByText("± 10.6pp (1 s.e.)")).toBeInTheDocument();
  });
});
