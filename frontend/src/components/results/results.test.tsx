import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PortfolioResultsPanel } from "./PortfolioResultsPanel";
import { TradeLogTable } from "./TradeLogTable";
import { candles, portfolioResult } from "../../test/portfolioFixture";

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
