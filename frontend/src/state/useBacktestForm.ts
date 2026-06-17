import { useCallback, useEffect, useMemo, useState } from "react";
import type {
  BacktestRequest,
  Candle,
  StrategyMeta,
  StrategyRequest,
  StrategyType,
} from "../types/api";

export interface BacktestForm {
  // Data source
  candles: Candle[];
  fileName: string | null;
  setCandles: (candles: Candle[], fileName: string) => void;

  // Strategy
  strategyType: StrategyType | null;
  setStrategyType: (type: StrategyType) => void;
  params: Record<string, number>;
  setParam: (name: string, value: number) => void;
  currentStrategyMeta: StrategyMeta | null;

  // Costs
  initialCash: number;
  commission: number;
  slippagePct: number; // stored as fraction (0–1)
  setInitialCash: (v: number) => void;
  setCommission: (v: number) => void;
  setSlippagePct: (v: number) => void;

  // Derived
  buildRequest: () => BacktestRequest | null;
}

function seedParams(meta: StrategyMeta | undefined): Record<string, number> {
  const out: Record<string, number> = {};
  meta?.params.forEach((p) => {
    out[p.name] = p.default;
  });
  return out;
}

/**
 * Central form state for the config panel. Seeds strategy params from the
 * registry defaults and re-seeds them whenever the selected strategy changes.
 */
export function useBacktestForm(strategies: StrategyMeta[]): BacktestForm {
  const [candles, setCandlesState] = useState<Candle[]>([]);
  const [fileName, setFileName] = useState<string | null>(null);
  const [strategyType, setStrategyTypeState] = useState<StrategyType | null>(
    null,
  );
  const [params, setParams] = useState<Record<string, number>>({});
  const [initialCash, setInitialCash] = useState(10_000);
  const [commission, setCommission] = useState(0);
  const [slippagePct, setSlippagePct] = useState(0);

  // Once the registry loads, default to the first strategy and seed its params.
  useEffect(() => {
    if (strategyType === null && strategies.length > 0) {
      setStrategyTypeState(strategies[0].type);
      setParams(seedParams(strategies[0]));
    }
  }, [strategies, strategyType]);

  const currentStrategyMeta = useMemo(
    () => strategies.find((s) => s.type === strategyType) ?? null,
    [strategies, strategyType],
  );

  const setCandles = useCallback((next: Candle[], name: string) => {
    setCandlesState(next);
    setFileName(name);
  }, []);

  const setStrategyType = useCallback(
    (type: StrategyType) => {
      setStrategyTypeState(type);
      setParams(seedParams(strategies.find((s) => s.type === type)));
    },
    [strategies],
  );

  const setParam = useCallback((name: string, value: number) => {
    setParams((prev) => ({ ...prev, [name]: value }));
  }, []);

  const buildRequest = useCallback((): BacktestRequest | null => {
    if (candles.length === 0 || !strategyType) return null;

    let strategy: StrategyRequest;
    if (strategyType === "rsi") {
      strategy = { type: "rsi", period: params.period };
    } else {
      strategy = {
        type: strategyType,
        short_window: params.short_window,
        long_window: params.long_window,
      };
    }

    return {
      strategy,
      candles,
      initial_cash: initialCash,
      commission,
      slippage_pct: slippagePct,
    };
  }, [candles, strategyType, params, initialCash, commission, slippagePct]);

  return {
    candles,
    fileName,
    setCandles,
    strategyType,
    setStrategyType,
    params,
    setParam,
    currentStrategyMeta,
    initialCash,
    commission,
    slippagePct,
    setInitialCash,
    setCommission,
    setSlippagePct,
    buildRequest,
  };
}
