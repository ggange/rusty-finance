import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { TradingConsole } from "./TradingConsole";
import { useTradeConsole } from "../../hooks/useTradeConsole";
import {
  consoleRoutes,
  drifting,
  emptyLimits,
  errorResponse,
  killSwitchOn,
  liveBroker,
  mockFetch,
} from "../../test/mockApi";
import type { Dataset, StrategyMeta } from "../../types/api";

const datasets: Dataset[] = [
  { name: "MSFT.csv", symbol: "MSFT", rows: 1600, start: "2020-01-02", end: "2026-07-31" },
];

const strategies: StrategyMeta[] = [
  {
    type: "rsi",
    name: "RSI",
    description: "Mean reversion",
    params: [{ name: "period", type: "integer", default: 14, min: 2 }],
  },
];

/** Renders the console against the real hook, so wiring is exercised too. */
function Harness() {
  const c = useTradeConsole(true);
  return (
    <TradingConsole
      console={c}
      datasets={datasets}
      strategies={strategies}
      engineAvailable
    />
  );
}

/** The broker name renders in both the status strip and the run controls. */
async function waitForBroker(name: string) {
  await waitFor(() => expect(screen.getAllByText(name).length).toBeGreaterThan(0));
}

async function renderConsole(overrides: Record<string, unknown> = {}) {
  const fetchMock = mockFetch(consoleRoutes(overrides));
  render(<Harness />);
  await waitForBroker("paper_sim");
  return fetchMock;
}

describe("TradingConsole", () => {
  it("loads every part of the trading state on mount", async () => {
    const fetchMock = await renderConsole();

    const paths = fetchMock.mock.calls.map((c) => String(c[0]));
    for (const route of [
      "/trade/plans",
      "/trade/broker",
      "/trade/schedule",
      "/trade/killswitch",
      "/trade/limits",
      "/trade/positions",
      "/trade/orders",
      "/trade/intents",
      "/trade/soak",
      "/trade/reconcile",
    ]) {
      expect(paths.some((p) => p.includes(route))).toBe(true);
    }
  });

  it("opens on positions and shows the ledger", async () => {
    await renderConsole();
    expect(screen.getByText("23.7791")).toBeInTheDocument();
    expect(screen.getByText("$9,999.11")).toBeInTheDocument();
  });

  it("switches to the orders view", async () => {
    const user = userEvent.setup();
    await renderConsole();

    await user.click(screen.getByRole("button", { name: /^Orders/ }));

    expect(screen.getByText("max_position_value exceeded")).toBeInTheDocument();
  });

  it("switches to the soak view", async () => {
    const user = userEvent.setup();
    await renderConsole();

    await user.click(screen.getByRole("button", { name: /^Soak/ }));

    expect(screen.getByText("95.0%")).toBeInTheDocument();
  });

  it("reports drift in the strip and the reconcile view", async () => {
    const user = userEvent.setup();
    await renderConsole({ "/trade/reconcile": drifting });

    expect(screen.getByText("1 drifting")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^Reconcile/ }));
    expect(screen.getByText(/investigate before trusting/i)).toBeInTheDocument();
  });

  it("shows both safety banners when a live broker is halted with no limits", async () => {
    mockFetch(
      consoleRoutes({
        "/trade/broker": liveBroker,
        "/trade/killswitch": killSwitchOn,
        "/trade/limits": emptyLimits,
      }),
    );
    render(<Harness />);

    await waitForBroker("alpaca");
    // the strip's banner, distinct from the run-controls warning
    expect(screen.getByText(/no orders will reach the broker/i)).toBeInTheDocument();
    expect(screen.getByText(/fails closed/i)).toBeInTheDocument();
  });

  it("surfaces a failed action without wiping the loaded state", async () => {
    const user = userEvent.setup();
    await renderConsole({
      // GET still works; only the write fails
      "POST /trade/killswitch": errorResponse(500, { detail: "db is locked" }),
    });

    await user.click(screen.getByRole("button", { name: /engage kill switch/i }));

    await waitFor(() => expect(screen.getByText("db is locked")).toBeInTheDocument());
    // the console is still showing what it loaded before the failure
    expect(screen.getAllByText("paper_sim").length).toBeGreaterThan(0);
  });

  it("dismisses the action error", async () => {
    const user = userEvent.setup();
    await renderConsole({
      // GET still works; only the write fails
      "POST /trade/killswitch": errorResponse(500, { detail: "db is locked" }),
    });

    await user.click(screen.getByRole("button", { name: /engage kill switch/i }));
    await waitFor(() => expect(screen.getByText("db is locked")).toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: /dismiss/i }));
    expect(screen.queryByText("db is locked")).not.toBeInTheDocument();
  });

  it("keeps the rest of the console usable when one endpoint is down", async () => {
    mockFetch(consoleRoutes({ "/trade/soak": errorResponse(500, { detail: "soak blew up" }) }));
    render(<Harness />);

    // the failure is named…
    await waitFor(() => expect(screen.getByText(/soak blew up/)).toBeInTheDocument());
    // …but the safety-critical state still rendered
    await waitForBroker("paper_sim");
    expect(screen.getByText(/in sync · 2 symbols/)).toBeInTheDocument();
    expect(screen.getByText("released")).toBeInTheDocument();
  });

  it("refetches on demand", async () => {
    const user = userEvent.setup();
    const fetchMock = await renderConsole();
    const before = fetchMock.mock.calls.length;

    await user.click(screen.getByRole("button", { name: /^refresh$/i }));

    await waitFor(() => expect(fetchMock.mock.calls.length).toBeGreaterThan(before));
  });
});
