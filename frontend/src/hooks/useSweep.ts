import { useCallback, useState } from "react";
import { api } from "../lib/apiClient";
import type { SelectionCorrection, SweepPoint, SweepRequest } from "../types/api";

export type SweepStatus = "idle" | "loading" | "success" | "error";

interface UseSweep {
  results: SweepPoint[];
  selection: SelectionCorrection | null;
  status: SweepStatus;
  error: string | null;
  run: (req: SweepRequest) => Promise<void>;
}

export function useSweep(): UseSweep {
  const [results, setResults] = useState<SweepPoint[]>([]);
  const [selection, setSelection] = useState<SelectionCorrection | null>(null);
  const [status, setStatus] = useState<SweepStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(async (req: SweepRequest) => {
    setStatus("loading");
    setError(null);
    // Cleared up front: a stale correction beside a fresh grid would describe a
    // search that is no longer on screen.
    setSelection(null);
    try {
      const res = await api.sweep(req);
      setResults(res.results);
      setSelection(res.selection ?? null);
      setStatus("success");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  }, []);

  return { results, selection, status, error, run };
}
