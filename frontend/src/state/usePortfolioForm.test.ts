import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { usePortfolioForm } from "./usePortfolioForm";
import type { Candle, StrategyMeta } from "../types/api";

const STRATEGIES: StrategyMeta[] = [
  {
    type: "ma_ema",
    name: "EMA Crossover",
    description: "",
    params: [
      { name: "short_window", type: "integer", default: 12, min: 1 },
      { name: "long_window", type: "integer", default: 26, min: 1 },
    ],
  },
  {
    type: "rsi",
    name: "RSI",
    description: "",
    params: [{ name: "period", type: "integer", default: 14, min: 2 }],
  },
];

const CANDLES: Candle[] = [
  { date: "2026-01-02", open: 1, high: 2, low: 0.5, close: 1.5, volume: 100 },
  { date: "2026-01-03", open: 1.5, high: 2.5, low: 1, close: 2, volume: 120 },
];

function setup() {
  return renderHook(() => usePortfolioForm(STRATEGIES));
}

describe("usePortfolioForm.buildRequest", () => {
  it("returns null while no asset has data — nothing to run", () => {
    const { result } = setup();
    expect(result.current.buildRequest()).toBeNull();
  });

  it("excludes assets that have no data source", () => {
    const { result } = setup();

    act(() => {
      result.current.addAsset();
    });
    act(() => {
      result.current.setAssetSourceDataset(
        result.current.assets[0].id,
        "MSFT.csv",
        CANDLES,
        "MSFT",
      );
    });

    const req = result.current.buildRequest();
    expect(req).not.toBeNull();
    expect(req!.assets).toHaveLength(1);
    expect(req!.assets[0].symbol).toBe("MSFT");
  });

  it("sends a dataset reference by name, not the candles it already loaded", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(
        result.current.assets[0].id,
        "MSFT.csv",
        CANDLES,
        "MSFT",
      );
    });

    expect(result.current.buildRequest()!.assets[0].source).toEqual({
      kind: "dataset",
      name: "MSFT.csv",
    });
  });

  it("inlines candles for an uploaded CSV", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceInline(result.current.assets[0].id, "custom.csv", CANDLES);
    });

    expect(result.current.buildRequest()!.assets[0].source).toEqual({
      kind: "inline",
      candles: CANDLES,
    });
  });

  it("names an unnamed asset rather than sending an empty symbol", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "");
    });
    act(() => {
      result.current.updateAsset(result.current.assets[0].id, { symbol: "" });
    });

    expect(result.current.buildRequest()!.assets[0].symbol).toBe("ASSET");
  });

  it("carries the strategy type and its parameters", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "X");
    });
    act(() => {
      result.current.setAssetStrategyType(result.current.assets[0].id, "rsi");
    });
    act(() => {
      result.current.setAssetParam(result.current.assets[0].id, "period", 22);
    });

    expect(result.current.buildRequest()!.assets[0].strategy).toEqual({
      type: "rsi",
      period: 22,
    });
  });

  it("omits benchmark and rebalance keys when unset", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "X");
    });

    const req = result.current.buildRequest()!;
    expect(req).not.toHaveProperty("benchmark_symbol");
    expect(req).not.toHaveProperty("rebalance");
  });

  it("includes benchmark and rebalance once configured", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "X");
    });
    act(() => {
      result.current.setBenchmarkSymbol("SPY.csv");
      result.current.setRebalanceConfig({ frequency: { kind: "quarterly" } });
    });

    const req = result.current.buildRequest()!;
    expect(req.benchmark_symbol).toBe("SPY.csv");
    expect(req.rebalance).toEqual({ frequency: { kind: "quarterly" } });
  });

  it("passes costs and fill timing straight through", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "X");
    });
    act(() => {
      result.current.setInitialCash(50_000);
      result.current.setCommission(1.5);
      result.current.setSlippagePct(0.001);
      result.current.setFillTiming("close");
    });

    const req = result.current.buildRequest()!;
    expect(req.initial_cash).toBe(50_000);
    expect(req.commission).toBe(1.5);
    expect(req.slippage_pct).toBe(0.001);
    expect(req.fill_timing).toBe("close");
  });

  it("resets params to the new strategy's defaults on a type change", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "X");
    });
    act(() => {
      result.current.setAssetParam(result.current.assets[0].id, "short_window", 5);
    });
    act(() => {
      result.current.setAssetStrategyType(result.current.assets[0].id, "rsi");
    });

    expect(result.current.buildRequest()!.assets[0].strategy).toEqual({
      type: "rsi",
      period: 14,
    });
  });

  it("omits weight_policy while weights are manual", () => {
    // Manual is the API default, so sending it explicitly is noise.
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "X");
    });

    expect(result.current.buildRequest()!).not.toHaveProperty("weight_policy");
  });

  it("sends a static policy with its optimizer settings", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "X");
    });
    act(() => {
      result.current.setWeightPolicy({
        kind: "static",
        warmup: 120,
        optimizer: { objective: "min_variance", shrinkage: 0.3, max_weight: 0.4 },
      });
    });

    expect(result.current.buildRequest()!.weight_policy).toEqual({
      kind: "static",
      warmup: 120,
      optimizer: { objective: "min_variance", shrinkage: 0.3, max_weight: 0.4 },
    });
  });

  it("sends a dynamic policy with its lookback", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "X");
    });
    act(() => {
      result.current.setWeightPolicy({
        kind: "dynamic",
        lookback: 252,
        optimizer: { objective: "risk_parity", shrinkage: 0.2, max_weight: null },
      });
    });

    const policy = result.current.buildRequest()!.weight_policy;
    expect(policy).toMatchObject({ kind: "dynamic", lookback: 252 });
  });

  it("drops the policy again when switched back to manual", () => {
    const { result } = setup();
    act(() => {
      result.current.setAssetSourceDataset(result.current.assets[0].id, "X.csv", CANDLES, "X");
    });
    act(() => {
      result.current.setWeightPolicy({
        kind: "dynamic",
        lookback: 60,
        optimizer: { objective: "risk_parity", shrinkage: 0.2, max_weight: null },
      });
    });
    act(() => {
      result.current.setWeightPolicy({ kind: "manual" });
    });

    expect(result.current.buildRequest()!).not.toHaveProperty("weight_policy");
  });

  it("removes an asset by id", () => {
    const { result } = setup();
    act(() => {
      result.current.addAsset();
    });
    const target = result.current.assets[1].id;
    act(() => {
      result.current.removeAsset(target);
    });

    expect(result.current.assets).toHaveLength(1);
    expect(result.current.assets.some((a) => a.id === target)).toBe(false);
  });
});
