import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { RunControls } from "./RunControls";
import {
  inSync,
  killSwitchOff,
  liveBroker,
  mockFetch,
  paperBroker,
  passthroughAct,
  positions,
} from "../../test/mockApi";
import type { BrokerInfo } from "../../types/api";

const tickResult = {
  plan_id: "default",
  results: [],
  positions,
  limits: { max_position_value: null, max_daily_loss: null, max_daily_orders: null },
  kill_switch: killSwitchOff,
  synced_orders: [],
  reconciliation: inSync,
};

function renderControls(broker: BrokerInfo = paperBroker, engaged = false) {
  const onTick = vi.fn();
  render(
    <RunControls
      planId="default"
      broker={broker}
      killSwitchEngaged={engaged}
      busy={false}
      act={passthroughAct}
      onTick={onTick}
    />,
  );
  return { onTick };
}

describe("RunControls", () => {
  it("does not submit on the first click — it asks first", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/tick": tickResult });
    renderControls();

    await user.click(screen.getByRole("button", { name: /tick this plan/i }));

    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: /confirm tick/i })).toBeInTheDocument();
  });

  it("submits the tick only after confirmation, and hands the result back", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/tick": tickResult });
    const { onTick } = renderControls();

    await user.click(screen.getByRole("button", { name: /tick this plan/i }));
    await user.click(screen.getByRole("button", { name: /confirm tick/i }));

    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain("/trade/tick");
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ plan_id: "default" });
    expect(onTick).toHaveBeenCalledWith(tickResult);
  });

  it("abandons the action on cancel", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/tick": tickResult });
    renderControls();

    await user.click(screen.getByRole("button", { name: /tick this plan/i }));
    await user.click(screen.getByRole("button", { name: /cancel/i }));

    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: /confirm tick/i })).not.toBeInTheDocument();
  });

  it("passes the refresh choice through to the cycle endpoint", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/schedule/run": { plans_run: 1 } });
    renderControls();

    await user.click(screen.getByRole("button", { name: /run scheduled cycle/i }));
    await user.click(screen.getByLabelText(/refresh market data first/i)); // default on → off
    await user.click(screen.getByRole("button", { name: /confirm cycle/i }));

    expect(String(fetchMock.mock.calls[0][0])).toContain("refresh=false");
  });

  it("warns that a live venue is on the other end", () => {
    renderControls(liveBroker);
    expect(screen.getByText(/a LIVE venue/i)).toBeInTheDocument();
  });

  it("styles the confirm button as destructive when the broker is live", async () => {
    const user = userEvent.setup();
    mockFetch({ "/trade/tick": tickResult });
    renderControls(liveBroker);

    await user.click(screen.getByRole("button", { name: /tick this plan/i }));

    expect(screen.getByRole("button", { name: /confirm tick/i }).className).toContain(
      "bg-rose-600",
    );
  });

  it("keeps the confirm button non-destructive on a paper broker", async () => {
    const user = userEvent.setup();
    mockFetch({ "/trade/tick": tickResult });
    renderControls(paperBroker);

    await user.click(screen.getByRole("button", { name: /tick this plan/i }));

    expect(screen.getByRole("button", { name: /confirm tick/i }).className).toContain(
      "bg-sky-500",
    );
  });

  it("says so when the kill switch will block every order", () => {
    renderControls(paperBroker, true);
    expect(screen.getByText(/every order will be blocked/i)).toBeInTheDocument();
  });
});
