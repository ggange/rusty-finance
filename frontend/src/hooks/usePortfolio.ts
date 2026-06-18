import { useCallback, useState } from "react";
import { api } from "../lib/apiClient";
import type { PortfolioRequest, PortfolioResponse } from "../types/api";

export type PortfolioStatus = "idle" | "loading" | "success" | "error";

interface UsePortfolio {
  result: PortfolioResponse | null;
  status: PortfolioStatus;
  error: string | null;
  run: (req: PortfolioRequest) => Promise<void>;
}

/** Imperative POST /portfolio with request-status tracking. */
export function usePortfolio(): UsePortfolio {
  const [result, setResult] = useState<PortfolioResponse | null>(null);
  const [status, setStatus] = useState<PortfolioStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(async (req: PortfolioRequest) => {
    setStatus("loading");
    setError(null);
    try {
      const res = await api.portfolio(req);
      setResult(res);
      setStatus("success");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  }, []);

  return { result, status, error, run };
}
