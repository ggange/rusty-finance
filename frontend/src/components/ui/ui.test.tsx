import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Button } from "./Button";
import { Input } from "./Input";
import { Select } from "./Select";
import { StatusPill, orderTone } from "./StatusPill";
import { Table, Td, Th, Tr } from "./Table";

describe("Button", () => {
  it("does not fire while loading", async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    render(
      <Button loading onClick={onClick}>
        Save
      </Button>,
    );

    await user.click(screen.getByRole("button"));
    expect(onClick).not.toHaveBeenCalled();
  });

  it("defaults to type=button so it never submits an enclosing form", () => {
    render(<Button>Go</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("type", "button");
  });

  it("applies one background per variant, so overrides do not conflict", () => {
    const { unmount } = render(<Button variant="danger">Halt</Button>);
    const danger = screen.getByRole("button").className;
    expect(danger).toContain("bg-rose-600");
    expect(danger).not.toContain("bg-sky-500");
    unmount();

    render(<Button variant="violet">Sweep</Button>);
    expect(screen.getByRole("button").className).toContain("bg-violet-600");
  });
});

describe("Input and Select", () => {
  it("uses the density scale rather than the native size attribute", () => {
    render(<Input size="sm" aria-label="qty" />);
    const input = screen.getByLabelText("qty");
    expect(input.className).toContain("px-2.5");
    expect(input).not.toHaveAttribute("size");
  });

  it("appends caller classes instead of replacing the base styling", () => {
    render(<Input className="w-24" aria-label="weight" />);
    const input = screen.getByLabelText("weight");
    expect(input.className).toContain("w-24");
    expect(input.className).toContain("rounded-md");
  });

  it("forwards value and change handling on Select", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <Select aria-label="pick" value="a" onChange={onChange}>
        <option value="a">A</option>
        <option value="b">B</option>
      </Select>,
    );

    await user.selectOptions(screen.getByLabelText("pick"), "b");
    expect(onChange).toHaveBeenCalled();
  });
});

describe("StatusPill", () => {
  it("maps order statuses onto sensible tones", () => {
    expect(orderTone("filled")).toBe("ok");
    expect(orderTone("rejected")).toBe("bad");
    expect(orderTone("accepted")).toBe("info");
    expect(orderTone("partially_filled")).toBe("info");
    expect(orderTone("canceled")).toBe("neutral");
    expect(orderTone("something_new")).toBe("neutral");
  });

  it("renders the tone's colour classes", () => {
    render(<StatusPill tone="bad">halted</StatusPill>);
    expect(screen.getByText("halted").className).toContain("rose");
  });
});

describe("Table", () => {
  it("emits real table semantics with aligned numeric cells", () => {
    render(
      <Table head={<Th align="right">Qty</Th>}>
        <Tr>
          <Td align="right" numeric>
            10
          </Td>
        </Tr>
      </Table>,
    );

    expect(screen.getByRole("columnheader", { name: "Qty" }).className).toContain(
      "text-right",
    );
    const cell = screen.getByRole("cell", { name: "10" });
    expect(cell.className).toContain("text-right");
    expect(cell.className).toContain("tabular-nums");
  });
});
