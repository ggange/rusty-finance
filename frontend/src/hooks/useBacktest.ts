import { useCallback, useState } from "react";
import { api } from "../lib/apiClient";
import type { BacktestRequest, BacktestResponse } from "../types/api";

export type BacktestStatus = "idle" | "loading" | "success" | "error";

interface UseBacktest {
  result: BacktestResponse | null;
  status: BacktestStatus;
  error: string | null;
  run: (req: BacktestRequest) => Promise<void>;
}

/** Imperative POST /backtest with request-status tracking. */
export function useBacktest(): UseBacktest {
  const [result, setResult] = useState<BacktestResponse | null>(null);
  const [status, setStatus] = useState<BacktestStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(async (req: BacktestRequest) => {
    setStatus("loading");
    setError(null);
    try {
      const res = await api.backtest(req);
      setResult(res);
      setStatus("success");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  }, []);

  return { result, status, error, run };
}
