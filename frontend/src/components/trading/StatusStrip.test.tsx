import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatusStrip } from "./StatusStrip";
import {
  drifting,
  emptyLimits,
  inSync,
  killSwitchOff,
  killSwitchOn,
  limitsForPlan,
  liveBroker,
  paperBroker,
  schedule,
} from "../../test/mockApi";

const base = {
  broker: paperBroker,
  schedule,
  killSwitch: killSwitchOff,
  limits: limitsForPlan,
  reconcile: inSync,
};

describe("StatusStrip", () => {
  it("shows a loud banner when the kill switch is engaged", () => {
    render(<StatusStrip {...base} killSwitch={killSwitchOn} />);
    expect(screen.getByText(/kill switch is ENGAGED/i)).toBeInTheDocument();
    expect(screen.getByText(/including exits/i)).toBeInTheDocument();
  });

  it("stays quiet when nothing is wrong", () => {
    render(<StatusStrip {...base} />);
    expect(screen.queryByText(/kill switch is ENGAGED/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/fails closed/i)).not.toBeInTheDocument();
  });

  it("warns when a live broker has no required limits", () => {
    render(<StatusStrip {...base} broker={liveBroker} limits={emptyLimits} />);
    expect(screen.getByText(/fails closed/i)).toBeInTheDocument();
  });

  it("does not warn about missing limits on a paper broker", () => {
    render(<StatusStrip {...base} broker={paperBroker} limits={emptyLimits} />);
    expect(screen.queryByText(/fails closed/i)).not.toBeInTheDocument();
  });

  it("marks a live broker distinctly from paper", () => {
    const { unmount } = render(<StatusStrip {...base} broker={liveBroker} />);
    expect(screen.getByText("LIVE")).toBeInTheDocument();
    unmount();

    render(<StatusStrip {...base} broker={paperBroker} />);
    expect(screen.getByText("paper")).toBeInTheDocument();
  });

  it("reports reconciliation drift instead of a healthy count", () => {
    render(<StatusStrip {...base} reconcile={drifting} />);
    expect(screen.getByText("1 drifting")).toBeInTheDocument();
  });

  it("reports in-sync with the number of symbols checked", () => {
    render(<StatusStrip {...base} reconcile={inSync} />);
    expect(screen.getByText(/in sync · 2 symbols/)).toBeInTheDocument();
  });

  it("renders the cron schedule in its configured timezone", () => {
    render(<StatusStrip {...base} />);
    expect(screen.getByText(/16:30 America\/New_York/)).toBeInTheDocument();
  });
});
