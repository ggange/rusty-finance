import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { WeightPolicyControls, defaultPolicy } from "./WeightPolicyControls";
import type { WeightPolicy } from "../../types/api";

function renderControls(value: WeightPolicy, hasRebalance = true) {
  const onChange = vi.fn();
  render(
    <WeightPolicyControls value={value} onChange={onChange} hasRebalance={hasRebalance} />,
  );
  return onChange;
}

/**
 * Stateful harness. The controls are fully controlled, so a test that types
 * several characters needs the value to feed back in — otherwise each keystroke
 * is applied to the original prop and only the last one survives.
 */
function Harness({ initial }: { initial: WeightPolicy }) {
  const [policy, setPolicy] = useState(initial);
  return (
    <>
      <WeightPolicyControls value={policy} onChange={setPolicy} hasRebalance />
      <output data-testid="policy">{JSON.stringify(policy)}</output>
    </>
  );
}

function currentPolicy(): WeightPolicy {
  return JSON.parse(screen.getByTestId("policy").textContent!) as WeightPolicy;
}

function optimizerOf(p: WeightPolicy) {
  if (p.kind === "manual") throw new Error("expected a solving policy");
  return p.optimizer;
}

describe("WeightPolicyControls", () => {
  it("hides the optimizer settings while weights are manual", () => {
    renderControls({ kind: "manual" });
    expect(screen.queryByLabelText(/objective/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/shrinkage/i)).not.toBeInTheDocument();
  });

  it("switches to a static policy with a sane default warm-up", async () => {
    const user = userEvent.setup();
    const onChange = renderControls({ kind: "manual" });

    await user.selectOptions(screen.getByLabelText(/weights/i), "static");

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "static", warmup: 252 }),
    );
  });

  it("offers a warm-up for static and a lookback for dynamic", () => {
    const { unmount } = render(
      <WeightPolicyControls value={defaultPolicy("static")} onChange={() => {}} hasRebalance />,
    );
    expect(screen.getByLabelText(/warm-up/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/lookback/i)).not.toBeInTheDocument();
    unmount();

    render(
      <WeightPolicyControls value={defaultPolicy("dynamic")} onChange={() => {}} hasRebalance />,
    );
    expect(screen.getByLabelText(/lookback/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/warm-up/i)).not.toBeInTheDocument();
  });

  it("warns that max Sharpe leans on noisy mean returns", async () => {
    const user = userEvent.setup();
    render(<Harness initial={defaultPolicy("dynamic")} />);
    expect(screen.queryByText(/noisier to estimate/i)).not.toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText(/objective/i), "max_sharpe");

    expect(screen.getByText(/noisier to estimate/i)).toBeInTheDocument();
  });

  it("does not warn for the covariance-only objectives", async () => {
    const user = userEvent.setup();
    render(<Harness initial={defaultPolicy("dynamic")} />);

    for (const objective of ["min_variance", "inverse_volatility", "equal_weight"]) {
      await user.selectOptions(screen.getByLabelText(/objective/i), objective);
      expect(screen.queryByText(/noisier to estimate/i)).not.toBeInTheDocument();
    }
  });

  it("converts the position cap from percent to a fraction", async () => {
    const user = userEvent.setup();
    render(<Harness initial={defaultPolicy("dynamic")} />);

    await user.type(screen.getByLabelText(/max position/i), "40");

    expect(optimizerOf(currentPolicy()).max_weight).toBeCloseTo(0.4, 10);
  });

  it("treats a blank cap as uncapped", async () => {
    const user = userEvent.setup();
    const policy = defaultPolicy("dynamic");
    optimizerOf(policy).max_weight = 0.5;
    render(<Harness initial={policy} />);

    await user.clear(screen.getByLabelText(/max position/i));

    expect(optimizerOf(currentPolicy()).max_weight).toBeNull();
  });

  it("clamps shrinkage into [0, 1]", async () => {
    const user = userEvent.setup();
    render(<Harness initial={defaultPolicy("dynamic")} />);

    await user.clear(screen.getByLabelText(/shrinkage/i));
    await user.type(screen.getByLabelText(/shrinkage/i), "5");

    expect(optimizerOf(currentPolicy()).shrinkage).toBeLessThanOrEqual(1);
  });

  it("keeps the lookback a whole number of bars", async () => {
    const user = userEvent.setup();
    render(<Harness initial={defaultPolicy("dynamic")} />);

    await user.clear(screen.getByLabelText(/lookback/i));
    await user.type(screen.getByLabelText(/lookback/i), "120");

    const policy = currentPolicy();
    if (policy.kind !== "dynamic") throw new Error("expected dynamic");
    expect(policy.lookback).toBe(120);
    expect(Number.isInteger(policy.lookback)).toBe(true);
  });

  it("preserves optimizer settings when only the window changes", async () => {
    const user = userEvent.setup();
    render(<Harness initial={defaultPolicy("dynamic")} />);

    await user.selectOptions(screen.getByLabelText(/objective/i), "min_variance");
    await user.clear(screen.getByLabelText(/lookback/i));
    await user.type(screen.getByLabelText(/lookback/i), "90");

    expect(optimizerOf(currentPolicy()).objective).toBe("min_variance");
  });

  it("tells the user monthly is assumed when dynamic has no schedule", () => {
    renderControls(defaultPolicy("dynamic"), false);
    expect(screen.getByText(/monthly is used/i)).toBeInTheDocument();
  });

  it("stays quiet about the schedule when one is configured", () => {
    renderControls(defaultPolicy("dynamic"), true);
    expect(screen.queryByText(/monthly is used/i)).not.toBeInTheDocument();
  });
});
