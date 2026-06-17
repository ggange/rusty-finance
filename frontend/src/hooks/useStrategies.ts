import { useEffect, useState } from "react";
import { api } from "../lib/apiClient";
import type { HealthResponse, StrategyMeta } from "../types/api";

interface UseStrategies {
  strategies: StrategyMeta[];
  health: HealthResponse | null;
  loading: boolean;
  error: string | null;
}

/** Load the strategy registry and engine health once on mount. */
export function useStrategies(): UseStrategies {
  const [strategies, setStrategies] = useState<StrategyMeta[]>([]);
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.strategies(), api.health()])
      .then(([s, h]) => {
        if (cancelled) return;
        setStrategies(s.strategies);
        setHealth(h);
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

  return { strategies, health, loading, error };
}
