import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SolvedWeightsPanel } from "./SolvedWeightsPanel";
import type { WeightSnapshot } from "../../types/api";

const SYMBOLS = ["MSFT", "SPY"];

const staticHistory: WeightSnapshot[] = [
  {
    date: "2021-01-04",
    weights: [0.3, 0.7],
    expected_volatility: 0.1842,
    risk_contribution: [0.25, 0.75],
  },
];

const dynamicHistory: WeightSnapshot[] = [
  ...staticHistory,
  {
    date: "2021-02-01",
    weights: [0.45, 0.55],
    expected_volatility: 0.2011,
    risk_contribution: [0.5, 0.5],
  },
];

describe("SolvedWeightsPanel", () => {
  it("renders nothing when no policy solved anything", () => {
    const { container } = render(<SolvedWeightsPanel symbols={SYMBOLS} history={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("labels a single solve as solved once", () => {
    render(<SolvedWeightsPanel symbols={SYMBOLS} history={staticHistory} />);
    expect(screen.getByText("solved once")).toBeInTheDocument();
    expect(screen.getByText(/effective 2021-01-04/)).toBeInTheDocument();
  });

  it("counts repeated solves and shows the date span", () => {
    render(<SolvedWeightsPanel symbols={SYMBOLS} history={dynamicHistory} />);
    expect(screen.getByText("2 solves")).toBeInTheDocument();
    expect(screen.getByText("2021-01-04 → 2021-02-01")).toBeInTheDocument();
  });

  it("shows the latest allocation, not the first", () => {
    render(<SolvedWeightsPanel symbols={SYMBOLS} history={dynamicHistory} />);
    expect(screen.getByText("45.0%")).toBeInTheDocument();
    expect(screen.getByText("55.0%")).toBeInTheDocument();
    expect(screen.queryByText("30.0%")).not.toBeInTheDocument();
  });

  it("names each asset alongside its weight", () => {
    render(<SolvedWeightsPanel symbols={SYMBOLS} history={dynamicHistory} />);
    expect(screen.getByText("MSFT")).toBeInTheDocument();
    expect(screen.getByText("SPY")).toBeInTheDocument();
  });

  it("shows risk contribution, which is what risk parity equalizes", () => {
    render(<SolvedWeightsPanel symbols={SYMBOLS} history={dynamicHistory} />);
    expect(screen.getAllByText("50.0%")).toHaveLength(2);
  });

  it("labels the volatility as predicted, not realized", () => {
    render(<SolvedWeightsPanel symbols={SYMBOLS} history={dynamicHistory} />);
    expect(screen.getByText("20.1%")).toBeInTheDocument();
    expect(screen.getByText(/not what the run realized/i)).toBeInTheDocument();
  });

  it("states that dynamic solves could not see past their own date", () => {
    render(<SolvedWeightsPanel symbols={SYMBOLS} history={dynamicHistory} />);
    expect(screen.getByText(/no solve saw data from after its own date/i)).toBeInTheDocument();
  });

  it("states that a static solve only saw its warm-up", () => {
    render(<SolvedWeightsPanel symbols={SYMBOLS} history={staticHistory} />);
    expect(screen.getByText(/saw only the warm-up/i)).toBeInTheDocument();
  });

  it("falls back to a positional label when a symbol is missing", () => {
    render(<SolvedWeightsPanel symbols={["MSFT"]} history={dynamicHistory} />);
    expect(screen.getByText("Asset 2")).toBeInTheDocument();
  });

  it("copes with a snapshot that carries no risk contribution", () => {
    render(
      <SolvedWeightsPanel
        symbols={SYMBOLS}
        history={[{ date: "2021-01-04", weights: [0.5, 0.5] }]}
      />,
    );
    expect(screen.getAllByText("—")).toHaveLength(2);
  });
});
