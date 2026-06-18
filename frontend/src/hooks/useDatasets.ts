import { useEffect, useState } from "react";
import { api } from "../lib/apiClient";
import type { Dataset } from "../types/api";

interface UseDatasets {
  datasets: Dataset[];
  loading: boolean;
  error: string | null;
}

/** Load the server-side dataset catalog once on mount. */
export function useDatasets(): UseDatasets {
  const [datasets, setDatasets] = useState<Dataset[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .datasets()
      .then((r) => {
        if (!cancelled) setDatasets(r.datasets);
      })
      .catch((e: Error) => {
        if (!cancelled) setError(e.message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return { datasets, loading, error };
}
