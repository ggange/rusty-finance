import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  Candle,
  FillTiming,
  PortfolioRequest,
  RebalanceConfig,
  WeightPolicy,
  StrategyMeta,
  StrategyRequest,
  StrategyType,
} from "../types/api";

/** Where one asset's price data comes from. */
export type AssetSourceState =
  | { kind: "none" }
  | { kind: "dataset"; name: string }
  | { kind: "inline"; fileName: string };

/** A single configured asset within the portfolio. */
export interface AssetConfig {
  id: string;
  symbol: string;
  source: AssetSourceState;
  /** Resolved candles for charting (dataset-fetched or uploaded). */
  candles: Candle[];
  strategyType: StrategyType;
  params: Record<string, number>;
  /** Relative weight; normalized server-side. */
  weight: number;
}

export interface PortfolioForm {
  assets: AssetConfig[];
  addAsset: () => void;
  removeAsset: (id: string) => void;
  updateAsset: (id: string, patch: Partial<AssetConfig>) => void;
  setAssetStrategyType: (id: string, type: StrategyType) => void;
  setAssetParam: (id: string, name: string, value: number) => void;
  setAssetSourceDataset: (id: string, name: string, candles: Candle[], symbol?: string) => void;
  setAssetSourceInline: (id: string, fileName: string, candles: Candle[]) => void;

  // Global costs
  initialCash: number;
  commission: number;
  slippagePct: number; // fraction (0–1)
  setInitialCash: (v: number) => void;
  setCommission: (v: number) => void;
  setSlippagePct: (v: number) => void;

  // Benchmark
  benchmarkSymbol: string | null;
  setBenchmarkSymbol: (v: string | null) => void;

  // Rebalancing
  rebalanceConfig: RebalanceConfig | null;
  setRebalanceConfig: (v: RebalanceConfig | null) => void;
  weightPolicy: WeightPolicy;
  setWeightPolicy: (v: WeightPolicy) => void;

  // Execution timing
  fillTiming: FillTiming;
  setFillTiming: (v: FillTiming) => void;

  // Derived
  strategyMeta: (type: StrategyType) => StrategyMeta | undefined;
  buildRequest: () => PortfolioRequest | null;
}

function seedParams(meta: StrategyMeta | undefined): Record<string, number> {
  const out: Record<string, number> = {};
  meta?.params.forEach((p) => {
    out[p.name] = p.default;
  });
  return out;
}

function buildStrategy(type: StrategyType, params: Record<string, number>): StrategyRequest {
  if (type === "rsi") {
    return { type: "rsi", period: params.period };
  }
  if (type === "macd") {
    return { type: "macd", fast_period: params.fast_period,
      slow_period: params.slow_period, signal_period: params.signal_period };
  }
  if (type === "bollinger_bands") {
    return { type: "bollinger_bands", period: params.period, std_dev_mult: params.std_dev_mult };
  }
  return { type, short_window: params.short_window, long_window: params.long_window };
}

/**
 * Form state for a multi-asset portfolio. Each asset carries its own data
 * source, strategy, params, and weight. Strategy params are seeded from the
 * registry and re-seeded when an asset's strategy changes.
 */
export function usePortfolioForm(strategies: StrategyMeta[]): PortfolioForm {
  const [assets, setAssets] = useState<AssetConfig[]>([]);
  const [initialCash, setInitialCash] = useState(10_000);
  const [commission, setCommission] = useState(0);
  const [slippagePct, setSlippagePct] = useState(0);
  const [benchmarkSymbol, setBenchmarkSymbol] = useState<string | null>(null);
  const [rebalanceConfig, setRebalanceConfig] = useState<RebalanceConfig | null>(null);
  const [weightPolicy, setWeightPolicy] = useState<WeightPolicy>({ kind: "manual" });
  const [fillTiming, setFillTiming] = useState<FillTiming>("next_open");
  const idCounter = useRef(0);

  const strategyMeta = useCallback(
    (type: StrategyType) => strategies.find((s) => s.type === type),
    [strategies],
  );

  const makeAsset = useCallback((): AssetConfig => {
    idCounter.current += 1;
    const meta = strategies[0];
    return {
      id: `asset-${idCounter.current}`,
      symbol: "",
      source: { kind: "none" },
      candles: [],
      strategyType: meta.type,
      params: seedParams(meta),
      weight: 1,
    };
  }, [strategies]);

  // Seed one empty asset once the registry has loaded.
  useEffect(() => {
    if (strategies.length > 0 && assets.length === 0) {
      setAssets([makeAsset()]);
    }
  }, [strategies, assets.length, makeAsset]);

  const addAsset = useCallback(() => {
    if (strategies.length === 0) return;
    setAssets((prev) => [...prev, makeAsset()]);
  }, [strategies, makeAsset]);

  const removeAsset = useCallback((id: string) => {
    setAssets((prev) => (prev.length <= 1 ? prev : prev.filter((a) => a.id !== id)));
  }, []);

  const updateAsset = useCallback((id: string, patch: Partial<AssetConfig>) => {
    setAssets((prev) => prev.map((a) => (a.id === id ? { ...a, ...patch } : a)));
  }, []);

  const setAssetStrategyType = useCallback(
    (id: string, type: StrategyType) => {
      const meta = strategies.find((s) => s.type === type);
      setAssets((prev) =>
        prev.map((a) =>
          a.id === id ? { ...a, strategyType: type, params: seedParams(meta) } : a,
        ),
      );
    },
    [strategies],
  );

  const setAssetParam = useCallback((id: string, name: string, value: number) => {
    setAssets((prev) =>
      prev.map((a) =>
        a.id === id ? { ...a, params: { ...a.params, [name]: value } } : a,
      ),
    );
  }, []);

  const setAssetSourceDataset = useCallback(
    (id: string, name: string, candles: Candle[], symbol?: string) => {
      setAssets((prev) =>
        prev.map((a) =>
          a.id === id
            ? {
                ...a,
                source: { kind: "dataset", name },
                candles,
                symbol: a.symbol || symbol || name.replace(/\.csv$/i, ""),
              }
            : a,
        ),
      );
    },
    [],
  );

  const setAssetSourceInline = useCallback(
    (id: string, fileName: string, candles: Candle[]) => {
      setAssets((prev) =>
        prev.map((a) =>
          a.id === id
            ? {
                ...a,
                source: { kind: "inline", fileName },
                candles,
                symbol: a.symbol || fileName.replace(/\.csv$/i, ""),
              }
            : a,
        ),
      );
    },
    [],
  );

  const buildRequest = useCallback((): PortfolioRequest | null => {
    const ready = assets.filter(
      (a) => a.source.kind !== "none" && a.candles.length > 0,
    );
    if (ready.length === 0) return null;

    const req: PortfolioRequest = {
      assets: ready.map((a) => ({
        symbol: a.symbol || "ASSET",
        weight: a.weight,
        source:
          a.source.kind === "dataset"
            ? { kind: "dataset", name: a.source.name }
            : { kind: "inline", candles: a.candles },
        strategy: buildStrategy(a.strategyType, a.params),
      })),
      initial_cash: initialCash,
      commission,
      slippage_pct: slippagePct,
      fill_timing: fillTiming,
    };
    if (benchmarkSymbol) req.benchmark_symbol = benchmarkSymbol;
    if (rebalanceConfig) req.rebalance = rebalanceConfig;
    // Manual is the API default, so it is omitted rather than sent explicitly.
    if (weightPolicy.kind !== "manual") req.weight_policy = weightPolicy;
    return req;
  }, [assets, initialCash, commission, slippagePct, benchmarkSymbol, rebalanceConfig, weightPolicy, fillTiming]);

  const meta = useMemo(() => strategyMeta, [strategyMeta]);

  return {
    assets,
    addAsset,
    removeAsset,
    updateAsset,
    setAssetStrategyType,
    setAssetParam,
    setAssetSourceDataset,
    setAssetSourceInline,
    initialCash,
    commission,
    slippagePct,
    setInitialCash,
    setCommission,
    setSlippagePct,
    benchmarkSymbol,
    setBenchmarkSymbol,
    rebalanceConfig,
    setRebalanceConfig,
    weightPolicy,
    setWeightPolicy,
    fillTiming,
    setFillTiming,
    strategyMeta: meta,
    buildRequest,
  };
}
