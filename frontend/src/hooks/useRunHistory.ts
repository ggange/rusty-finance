import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/apiClient";
import type { RunListItem } from "../types/api";

export interface RunHistory {
  runs: RunListItem[];
  loading: boolean;
  refresh: () => void;
}

export function useRunHistory(): RunHistory {
  const [runs, setRuns] = useState<RunListItem[]>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(() => {
    setLoading(true);
    api
      .runs()
      .then((r) => setRuns(r.runs))
      .catch(() => setRuns([]))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { runs, loading, refresh };
}
