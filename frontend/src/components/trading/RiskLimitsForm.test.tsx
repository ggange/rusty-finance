import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { GLOBAL_LIMITS, RiskLimitsForm } from "./RiskLimitsForm";
import {
  emptyLimits,
  globalLimitsRow,
  limitsForPlan,
  mockFetch,
  passthroughAct,
} from "../../test/mockApi";
import type { LimitsForPlan } from "../../types/api";

function renderForm(limits: LimitsForPlan | null = limitsForPlan) {
  render(
    <RiskLimitsForm planId="default" limits={limits} busy={false} act={passthroughAct} />,
  );
}

function effective() {
  return within(screen.getByText(/effective for default/i).parentElement!);
}

describe("RiskLimitsForm", () => {
  it("shows the merged effective limits, not just one source", () => {
    renderForm(limitsForPlan);
    const eff = effective();
    // plan overrides position value; loss and order count fall through to global
    expect(eff.getByText("$5,000")).toBeInTheDocument();
    expect(eff.getByText("$1,000")).toBeInTheDocument();
    expect(eff.getByText("20")).toBeInTheDocument();
  });

  it("renders unset fields as unlimited rather than blank or zero", () => {
    renderForm(emptyLimits);
    expect(effective().getAllByText("unlimited")).toHaveLength(3);
  });

  it("seeds the form from the global row by default", () => {
    renderForm(limitsForPlan);
    expect(screen.getByLabelText(/max position value/i)).toHaveValue(
      globalLimitsRow.max_position_value,
    );
  });

  it("re-seeds from the plan row when the scope switches", async () => {
    const user = userEvent.setup();
    renderForm(limitsForPlan);

    await user.selectOptions(screen.getByLabelText(/applies to/i), "plan");

    expect(screen.getByLabelText(/max position value/i)).toHaveValue(5000);
    expect(screen.getByLabelText(/max daily loss/i)).toHaveValue(null);
  });

  it("saves against the global id when scoped globally", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/limits": globalLimitsRow });
    renderForm(limitsForPlan);

    await user.click(screen.getByRole("button", { name: /save limits/i }));

    const body = JSON.parse(fetchMock.mock.calls[0][1]!.body as string);
    expect(body.plan_id).toBe(GLOBAL_LIMITS);
  });

  it("saves against the plan id when scoped to the plan", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/limits": globalLimitsRow });
    renderForm(limitsForPlan);

    await user.selectOptions(screen.getByLabelText(/applies to/i), "plan");
    await user.click(screen.getByRole("button", { name: /save limits/i }));

    expect(JSON.parse(fetchMock.mock.calls[0][1]!.body as string).plan_id).toBe("default");
  });

  it("sends null for a blank field, meaning no limit", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/limits": globalLimitsRow });
    renderForm(limitsForPlan);

    await user.clear(screen.getByLabelText(/max daily orders/i));
    await user.click(screen.getByRole("button", { name: /save limits/i }));

    const body = JSON.parse(fetchMock.mock.calls[0][1]!.body as string);
    expect(body.max_daily_orders).toBeNull();
    expect(body.max_position_value).toBe(25000);
  });

  it("disables Clear when nothing is stored for the scope", () => {
    renderForm(emptyLimits);
    expect(screen.getByRole("button", { name: /clear/i })).toBeDisabled();
  });
});
