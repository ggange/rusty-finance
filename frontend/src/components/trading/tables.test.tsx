import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { IntentsTable } from "./IntentsTable";
import { OrdersTable } from "./OrdersTable";
import { PositionsTable } from "./PositionsTable";
import { ReconcilePanel } from "./ReconcilePanel";
import { SoakPanel } from "./SoakPanel";
import {
  drifting,
  inSync,
  intents,
  liveBroker,
  orders,
  paperBroker,
  positions,
  soak,
} from "../../test/mockApi";

describe("PositionsTable", () => {
  it("says flat rather than showing an empty grid", () => {
    render(<PositionsTable positions={[]} />);
    expect(screen.getByText(/flat — no open positions/i)).toBeInTheDocument();
  });

  it("derives cost basis from qty and average price", () => {
    render(<PositionsTable positions={positions} />);
    expect(screen.getByText("$9,999.11")).toBeInTheDocument();
  });

  it("keeps fractional share quantities instead of rounding them away", () => {
    render(<PositionsTable positions={positions} />);
    expect(screen.getByText("23.7791")).toBeInTheDocument();
  });

  it("renders the stored strategy blob readably", () => {
    render(<PositionsTable positions={positions} />);
    expect(screen.getByText("rsi(period=14)")).toBeInTheDocument();
  });
});

describe("OrdersTable", () => {
  it("distinguishes a filled order from a rejected one", () => {
    render(<OrdersTable orders={orders} openOnly={false} onOpenOnly={() => {}} />);
    expect(screen.getByText("filled")).toBeInTheDocument();
    expect(screen.getByText("rejected")).toBeInTheDocument();
  });

  it("surfaces the rejection reason so the guardrail is visible", () => {
    render(<OrdersTable orders={orders} openOnly={false} onOpenOnly={() => {}} />);
    expect(screen.getByText("max_position_value exceeded")).toBeInTheDocument();
  });

  it("shows filled against requested quantity", () => {
    render(<OrdersTable orders={orders} openOnly={false} onOpenOnly={() => {}} />);
    expect(screen.getByText("10 / 10")).toBeInTheDocument();
    expect(screen.getByText("0 / 5")).toBeInTheDocument();
  });

  it("toggles the open-only filter", async () => {
    const user = userEvent.setup();
    const onOpenOnly = vi.fn();
    render(<OrdersTable orders={orders} openOnly={false} onOpenOnly={onOpenOnly} />);

    await user.click(screen.getByLabelText(/open orders only/i));
    expect(onOpenOnly).toHaveBeenCalledWith(true);
  });

  it("explains an empty list differently when filtered", () => {
    render(<OrdersTable orders={[]} openOnly onOpenOnly={() => {}} />);
    expect(screen.getByText(/no open orders/i)).toBeInTheDocument();
  });
});

describe("IntentsTable", () => {
  it("keeps rejected intents visible, with the cause as readable prose", () => {
    render(<IntentsTable intents={intents} />);
    // the verb is a compact pill, the reason sits below it in full
    expect(screen.getByText("rejected")).toBeInTheDocument();
    expect(screen.getByText("max_position_value exceeded")).toBeInTheDocument();
  });

  it("renders a bare status with no trailing detail", () => {
    render(<IntentsTable intents={intents} />);
    expect(screen.getByText("filled")).toBeInTheDocument();
  });

  it("renders a dash for an entry with no realized PnL", () => {
    render(<IntentsTable intents={intents} />);
    expect(screen.getAllByText("—")).toHaveLength(2);
  });
});

describe("ReconcilePanel", () => {
  it("presents zero drift as a positive result, not as no data", () => {
    render(<ReconcilePanel reconcile={inSync} />);
    expect(screen.getByText(/ledger matches paper_sim/i)).toBeInTheDocument();
    expect(screen.getByText(/2 symbols checked, no drift/i)).toBeInTheDocument();
  });

  it("lists each drifting symbol with its delta", () => {
    render(<ReconcilePanel reconcile={drifting} />);
    expect(screen.getByText(/1 symbol drifting/i)).toBeInTheDocument();
    expect(screen.getByText("MSFT")).toBeInTheDocument();
    expect(screen.getByText("-3")).toBeInTheDocument();
  });
});

describe("SoakPanel", () => {
  it("reports fill rate and slippage", () => {
    render(<SoakPanel soak={soak} broker={paperBroker} />);
    expect(screen.getByText("95.0%")).toBeInTheDocument();
    // mean and worst both read 8 bps on the simulator
    expect(screen.getAllByText("8.00 bps")).toHaveLength(2);
    expect(screen.getByText("$1,420.56")).toBeInTheDocument();
  });

  it("states that simulated slippage is a configured value, not a measurement", () => {
    render(<SoakPanel soak={soak} broker={paperBroker} />);
    expect(screen.getByText(/not a market telling you something/i)).toBeInTheDocument();
  });

  it("drops the simulator caveat once the broker is live", () => {
    render(<SoakPanel soak={soak} broker={liveBroker} />);
    expect(
      screen.queryByText(/not a market telling you something/i),
    ).not.toBeInTheDocument();
  });

  it("explains that metrics need at least one order", () => {
    render(<SoakPanel soak={{ ...soak, orders: 0 }} broker={paperBroker} />);
    expect(screen.getByText(/no orders submitted yet/i)).toBeInTheDocument();
  });
});
