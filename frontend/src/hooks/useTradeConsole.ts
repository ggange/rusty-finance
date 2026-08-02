import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/apiClient";
import type {
  BrokerInfo,
  IntentRow,
  KillSwitch,
  LimitsForPlan,
  OrderRow,
  PositionRow,
  ReconcileResult,
  ScheduleStatus,
  SoakReport,
  TickResult,
  TradePlan,
} from "../types/api";

export type LoadStatus = "idle" | "loading" | "success" | "error";

/** Default polling interval. Slow enough to be a monitor, not a stress test. */
export const DEFAULT_POLL_MS = 15_000;

export interface TradeConsole {
  plans: TradePlan[];
  planId: string;
  setPlanId: (id: string) => void;

  broker: BrokerInfo | null;
  schedule: ScheduleStatus | null;
  killSwitch: KillSwitch | null;
  limits: LimitsForPlan | null;
  positions: PositionRow[];
  orders: OrderRow[];
  intents: IntentRow[];
  soak: SoakReport | null;
  reconcile: ReconcileResult | null;

  openOnly: boolean;
  setOpenOnly: (v: boolean) => void;

  status: LoadStatus;
  error: string | null;
  /** Error from the last mutating action (tick, plan save, limits, …). */
  actionError: string | null;
  clearActionError: () => void;
  busy: boolean;

  polling: boolean;
  setPolling: (v: boolean) => void;

  refresh: () => Promise<void>;
  /** Wraps a mutating call: tracks busy/error, then refreshes. */
  act: <T>(fn: () => Promise<T>) => Promise<T | null>;
  applyTick: (tick: TickResult) => void;
  lastTick: TickResult | null;
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * All trading server state in one place.
 *
 * The console is monitoring-first, so everything is fetched together and can
 * poll on an interval. Polling is opt-in and paused while a mutating action is
 * in flight, so a refresh can't overwrite the result of a tick mid-render.
 */
export function useTradeConsole(active: boolean): TradeConsole {
  const [plans, setPlans] = useState<TradePlan[]>([]);
  const [planId, setPlanId] = useState("default");
  const [broker, setBroker] = useState<BrokerInfo | null>(null);
  const [schedule, setSchedule] = useState<ScheduleStatus | null>(null);
  const [killSwitch, setKillSwitch] = useState<KillSwitch | null>(null);
  const [limits, setLimits] = useState<LimitsForPlan | null>(null);
  const [positions, setPositions] = useState<PositionRow[]>([]);
  const [orders, setOrders] = useState<OrderRow[]>([]);
  const [intents, setIntents] = useState<IntentRow[]>([]);
  const [soak, setSoak] = useState<SoakReport | null>(null);
  const [reconcile, setReconcile] = useState<ReconcileResult | null>(null);

  const [openOnly, setOpenOnly] = useState(false);
  const [status, setStatus] = useState<LoadStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [polling, setPolling] = useState(false);
  const [lastTick, setLastTick] = useState<TickResult | null>(null);

  // Guards against a slow response landing after the component unmounts or
  // after a newer refresh has already resolved.
  const requestSeq = useRef(0);
  const busyRef = useRef(false);

  const refresh = useCallback(async () => {
    const seq = ++requestSeq.current;
    setStatus((s) => (s === "idle" ? "loading" : s));

    // allSettled, not all: this is a monitoring surface, so one broken endpoint
    // must not blank out the kill switch and everything else next to it. Parts
    // that resolve are shown; the names of the parts that failed are reported.
    const failures: string[] = [];
    async function load<T>(name: string, fn: () => Promise<T>, apply: (v: T) => void) {
      try {
        apply(await fn());
      } catch (e) {
        failures.push(`${name} (${message(e)})`);
      }
    }

    await Promise.all([
      load("plans", () => api.trade.plans(), (r) => setPlans(r.plans)),
      load("broker", () => api.trade.broker(), setBroker),
      load("schedule", () => api.trade.schedule(), setSchedule),
      load("kill switch", () => api.trade.killSwitch(), setKillSwitch),
      load("limits", () => api.trade.limitsForPlan(planId), setLimits),
      load("positions", () => api.trade.positions(planId), (r) => setPositions(r.positions)),
      load("orders", () => api.trade.orders({ planId, openOnly }), (r) => setOrders(r.orders)),
      load("intents", () => api.trade.intents({ planId }), (r) => setIntents(r.intents)),
      load("soak", () => api.trade.soak(planId), setSoak),
      load("reconcile", () => api.trade.reconcile(planId), setReconcile),
    ]);

    if (seq !== requestSeq.current) return; // superseded
    if (failures.length > 0) {
      setError(failures.join("; "));
      setStatus("error");
    } else {
      setError(null);
      setStatus("success");
    }
  }, [planId, openOnly]);

  const act = useCallback(
    async <T,>(fn: () => Promise<T>): Promise<T | null> => {
      setBusy(true);
      busyRef.current = true;
      setActionError(null);
      try {
        const out = await fn();
        await refresh();
        return out;
      } catch (e) {
        setActionError(message(e));
        return null;
      } finally {
        setBusy(false);
        busyRef.current = false;
      }
    },
    [refresh],
  );

  /** Fold a tick response into state directly — it already carries most of it. */
  const applyTick = useCallback((tick: TickResult) => {
    setLastTick(tick);
    setPositions(tick.positions);
    setKillSwitch(tick.kill_switch);
    setReconcile(tick.reconciliation);
  }, []);

  useEffect(() => {
    if (!active) return;
    void refresh();
  }, [active, refresh]);

  useEffect(() => {
    if (!active || !polling) return;
    const id = window.setInterval(() => {
      if (!busyRef.current) void refresh();
    }, DEFAULT_POLL_MS);
    return () => window.clearInterval(id);
  }, [active, polling, refresh]);

  // Drop in-flight responses when the tab is left, so a late reply can't
  // repaint stale numbers on return.
  useEffect(() => {
    if (!active) requestSeq.current++;
  }, [active]);

  return {
    plans,
    planId,
    setPlanId,
    broker,
    schedule,
    killSwitch,
    limits,
    positions,
    orders,
    intents,
    soak,
    reconcile,
    openOnly,
    setOpenOnly,
    status,
    error,
    actionError,
    clearActionError: useCallback(() => setActionError(null), []),
    busy,
    polling,
    setPolling,
    refresh,
    act,
    applyTick,
    lastTick,
  };
}
