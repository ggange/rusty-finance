import { useCallback, useState } from "react";
import { api } from "../lib/apiClient";
import type { Metrics, WalkForwardFold, WalkForwardRequest } from "../types/api";

export type WalkForwardStatus = "idle" | "loading" | "success" | "error";

interface UseWalkForward {
  folds: WalkForwardFold[];
  /** Pooled out-of-sample metrics across all folds, when the run produced them. */
  oos: Metrics | null;
  status: WalkForwardStatus;
  error: string | null;
  run: (req: WalkForwardRequest) => Promise<void>;
}

export function useWalkForward(): UseWalkForward {
  const [folds, setFolds] = useState<WalkForwardFold[]>([]);
  const [oos, setOos] = useState<Metrics | null>(null);
  const [status, setStatus] = useState<WalkForwardStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(async (req: WalkForwardRequest) => {
    setStatus("loading");
    setError(null);
    try {
      const res = await api.walkForward(req);
      setFolds(res.folds);
      setOos(res.oos_metrics ?? null);
      setStatus("success");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  }, []);

  return { folds, oos, status, error, run };
}
