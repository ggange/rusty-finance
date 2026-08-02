import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { RunHistoryPanel } from "./RunHistoryPanel";
import { mockFetch } from "../../test/mockApi";
import type { RunListItem } from "../../types/api";

const runs: RunListItem[] = [
  {
    id: 3,
    created_at: "2026-08-01T20:30:00Z",
    kind: "scheduled_tick",
    config: { refresh: true, symbols: ["MSFT"], plan_ids: ["default"] },
  },
  {
    id: 2,
    created_at: "2026-08-01T12:00:00Z",
    kind: "portfolio",
    config: { assets: [{ symbol: "MSFT" }, { symbol: "NVDA" }] },
  },
];

function renderPanel(onLoad = vi.fn()) {
  render(
    <RunHistoryPanel
      history={{ runs, loading: false, refresh: vi.fn() }}
      onLoad={onLoad}
    />,
  );
  return onLoad;
}

describe("RunHistoryPanel", () => {
  it("labels a scheduled tick by its plan ids instead of calling it a backtest", () => {
    renderPanel();
    expect(screen.getByText("Tick · default")).toBeInTheDocument();
  });

  it("labels a portfolio run by its symbols", () => {
    renderPanel();
    expect(screen.getByText("MSFT, NVDA")).toBeInTheDocument();
  });

  it("disables scheduled ticks rather than offering a button that does nothing", () => {
    renderPanel();
    expect(screen.getByRole("button", { name: /Tick · default/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /MSFT, NVDA/ })).toBeEnabled();
  });

  it("loads the detail for a restorable run", async () => {
    const user = userEvent.setup();
    const detail = { ...runs[1], result: { equity_curve: [] } };
    mockFetch({ "/runs/2": detail });
    const onLoad = renderPanel();

    await user.click(screen.getByRole("button", { name: /MSFT, NVDA/ }));

    expect(onLoad).toHaveBeenCalledWith(detail);
  });
});
