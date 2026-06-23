import { useCallback, useState } from "react";
import { api } from "../lib/apiClient";
import type { SweepPoint, SweepRequest } from "../types/api";

export type SweepStatus = "idle" | "loading" | "success" | "error";

interface UseSweep {
  results: SweepPoint[];
  status: SweepStatus;
  error: string | null;
  run: (req: SweepRequest) => Promise<void>;
}

export function useSweep(): UseSweep {
  const [results, setResults] = useState<SweepPoint[]>([]);
  const [status, setStatus] = useState<SweepStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(async (req: SweepRequest) => {
    setStatus("loading");
    setError(null);
    try {
      const res = await api.sweep(req);
      setResults(res.results);
      setStatus("success");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  }, []);

  return { results, status, error, run };
}
