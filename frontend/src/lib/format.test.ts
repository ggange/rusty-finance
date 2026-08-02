import { describe, expect, it } from "vitest";
import { formatCurrency, formatPct, formatQty, formatStrategy, signClass } from "./format";

describe("formatQty", () => {
  it("keeps fractional shares — the loop sizes by cash, not by lot", () => {
    expect(formatQty(32.37188786847498)).toBe("32.3719");
    expect(formatQty(23.7791)).toBe("23.7791");
  });

  it("prints whole numbers without trailing zeros", () => {
    expect(formatQty(10)).toBe("10");
    expect(formatQty(0)).toBe("0");
  });

  it("keeps the sign for a negative drift delta", () => {
    expect(formatQty(-3.5)).toBe("-3.5");
  });

  it("renders a dash for a missing quantity", () => {
    expect(formatQty(null)).toBe("—");
    expect(formatQty(NaN)).toBe("—");
  });
});

describe("formatStrategy", () => {
  it("turns the stored params blob into something readable", () => {
    expect(formatStrategy('{"period": 5, "type": "rsi"}')).toBe("rsi(period=5)");
  });

  it("keeps every parameter", () => {
    expect(
      formatStrategy('{"type": "macd", "fast_period": 12, "slow_period": 26}'),
    ).toBe("macd(fast_period=12, slow_period=26)");
  });

  it("handles a strategy with no parameters", () => {
    expect(formatStrategy('{"type": "rsi"}')).toBe("rsi");
  });

  it("falls back to the raw value when it is not JSON", () => {
    expect(formatStrategy("rsi")).toBe("rsi");
    expect(formatStrategy("")).toBe("");
  });

  it("falls back when the JSON is not an object", () => {
    expect(formatStrategy("42")).toBe("42");
    expect(formatStrategy("null")).toBe("null");
  });
});

describe("existing formatters", () => {
  it("renders nulls as an em dash rather than NaN", () => {
    expect(formatPct(null)).toBe("—");
    expect(formatCurrency(null)).toBe("—");
  });

  it("colours by sign, with neutral for zero and null", () => {
    expect(signClass(1)).toContain("emerald");
    expect(signClass(-1)).toContain("rose");
    expect(signClass(0)).toContain("slate");
    expect(signClass(null)).toContain("slate");
  });
});
