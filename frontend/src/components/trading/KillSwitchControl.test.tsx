import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { KillSwitchControl, RELEASE_PHRASE } from "./KillSwitchControl";
import {
  killSwitchOff,
  killSwitchOn,
  mockFetch,
  passthroughAct,
} from "../../test/mockApi";

function renderControl(killSwitch = killSwitchOff) {
  render(
    <KillSwitchControl killSwitch={killSwitch} busy={false} act={passthroughAct} />,
  );
}

describe("KillSwitchControl", () => {
  it("engages in a single click — stopping is never harder than starting", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/killswitch": killSwitchOn });
    renderControl(killSwitchOff);

    await user.click(screen.getByRole("button", { name: /engage kill switch/i }));

    const body = JSON.parse(fetchMock.mock.calls[0][1]!.body as string);
    expect(body.engaged).toBe(true);
  });

  it("sends the typed reason with the halt", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/killswitch": killSwitchOn });
    renderControl(killSwitchOff);

    await user.type(screen.getByLabelText(/reason/i), "bad data feed");
    await user.click(screen.getByRole("button", { name: /engage kill switch/i }));

    const body = JSON.parse(fetchMock.mock.calls[0][1]!.body as string);
    expect(body).toEqual({ engaged: true, reason: "bad data feed" });
  });

  it("disables release until the confirmation phrase is typed", async () => {
    const user = userEvent.setup();
    mockFetch({ "/trade/killswitch": killSwitchOff });
    renderControl(killSwitchOn);

    const release = screen.getByRole("button", { name: /release halt/i });
    expect(release).toBeDisabled();

    await user.type(screen.getByLabelText(/type release to confirm/i), "REL");
    expect(release).toBeDisabled();

    await user.type(screen.getByLabelText(/type release to confirm/i), "EASE");
    expect(release).toBeEnabled();
  });

  it("does not release when the phrase is wrong", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/killswitch": killSwitchOff });
    renderControl(killSwitchOn);

    await user.type(screen.getByLabelText(/type release to confirm/i), "yes please");
    await user.click(screen.getByRole("button", { name: /release halt/i }));

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("releases once confirmed", async () => {
    const user = userEvent.setup();
    const fetchMock = mockFetch({ "/trade/killswitch": killSwitchOff });
    renderControl(killSwitchOn);

    await user.type(screen.getByLabelText(/type release to confirm/i), RELEASE_PHRASE);
    await user.click(screen.getByRole("button", { name: /release halt/i }));

    const body = JSON.parse(fetchMock.mock.calls[0][1]!.body as string);
    expect(body.engaged).toBe(false);
  });

  it("shows the stored reason while engaged", () => {
    renderControl(killSwitchOn);
    expect(screen.getByText("manual halt during review")).toBeInTheDocument();
    expect(screen.getByText("ENGAGED")).toBeInTheDocument();
  });

  it("offers no engage button while already engaged", () => {
    renderControl(killSwitchOn);
    expect(
      screen.queryByRole("button", { name: /engage kill switch/i }),
    ).not.toBeInTheDocument();
  });
});
