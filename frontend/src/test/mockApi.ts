import { vi } from "vitest";
import type {
  BrokerInfo,
  IntentRow,
  KillSwitch,
  LimitsForPlan,
  OrderRow,
  PositionRow,
  ReconcileResult,
  RiskLimitsRow,
  ScheduleStatus,
  SoakReport,
  TradePlan,
} from "../types/api";

/**
 * Install a fetch stub that answers by path.
 *
 * Routes match as substrings of the pathname, so callers key on "/trade/orders"
 * without repeating the /api prefix or the query string, and a route still
 * matches when the real call appends a path param (/trade/plans/default).
 *
 * A key may be prefixed with a method ("POST /trade/killswitch") to answer only
 * that verb — useful when a GET should succeed but the matching POST must fail.
 * Method-qualified keys are tried first; otherwise the longest match wins.
 *
 * An unmatched path throws, which surfaces as a visible test failure rather
 * than a silent undefined.
 */
export function mockFetch(routes: Record<string, unknown>) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const path = url.split("?")[0];
    const method = (init?.method ?? "GET").toUpperCase();

    const matches = (r: string) => {
      const [maybeMethod, ...rest] = r.split(" ");
      if (rest.length === 0) return path.includes(r);
      return maybeMethod.toUpperCase() === method && path.includes(rest.join(" "));
    };

    const key = Object.keys(routes)
      .filter(matches)
      // method-qualified keys are more specific, then longer paths
      .sort((a, b) => Number(b.includes(" ")) - Number(a.includes(" ")) || b.length - a.length)[0];
    if (key === undefined) {
      throw new Error(`mockFetch: no route for ${method} ${url}`);
    }
    const value = routes[key];
    if (value instanceof Response) return value;
    return new Response(JSON.stringify(value), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

/**
 * A stand-in for TradeConsole.act that just runs the call and swallows errors,
 * matching the real one's `T | null` contract. Written as a function rather
 * than a vi.fn because the prop is generic and a Mock erases that.
 */
export async function passthroughAct<T>(fn: () => Promise<T>): Promise<T | null> {
  try {
    return await fn();
  } catch {
    return null;
  }
}

/** A JSON error response, e.g. FastAPI's 503 or 422 envelopes. */
export function errorResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

// ─── Fixtures ────────────────────────────────────────────────────────────────

export const paperBroker: BrokerInfo = {
  kind: "paper_sim",
  slippage: 0.0008,
  fill_ratio: 1,
  name: "paper_sim",
  is_live: false,
};

export const liveBroker: BrokerInfo = {
  kind: "alpaca",
  slippage: 0,
  fill_ratio: 1,
  name: "alpaca",
  is_live: true,
};

export const killSwitchOff: KillSwitch = {
  engaged: false,
  reason: null,
  updated_at: "2026-08-01T16:30:00Z",
};

export const killSwitchOn: KillSwitch = {
  engaged: true,
  reason: "manual halt during review",
  updated_at: "2026-08-01T16:30:00Z",
};

export const globalLimitsRow: RiskLimitsRow = {
  plan_id: "__global__",
  max_position_value: 25_000,
  max_daily_loss: 1_000,
  max_daily_orders: 20,
  updated_at: "2026-08-01T12:00:00Z",
};

export const planLimitsRow: RiskLimitsRow = {
  plan_id: "default",
  max_position_value: 5_000,
  max_daily_loss: null,
  max_daily_orders: null,
  updated_at: "2026-08-01T12:00:00Z",
};

/** Plan overrides position value; the other two fall through to global. */
export const limitsForPlan: LimitsForPlan = {
  plan_id: "default",
  effective: {
    max_position_value: 5_000,
    max_daily_loss: 1_000,
    max_daily_orders: 20,
  },
  plan: planLimitsRow,
  global: globalLimitsRow,
};

export const emptyLimits: LimitsForPlan = {
  plan_id: "default",
  effective: {
    max_position_value: null,
    max_daily_loss: null,
    max_daily_orders: null,
  },
  plan: null,
  global: null,
};

export const schedule: ScheduleStatus = {
  enabled: true,
  running: true,
  refresh_data: true,
  cron: { day_of_week: "mon-fri", hour: 16, minute: 30, timezone: "America/New_York" },
  next_run: "2026-08-03T20:30:00Z",
  last_run: null,
};

export const inSync: ReconcileResult = {
  broker: "paper_sim",
  in_sync: true,
  checked: 2,
  drift: [],
};

export const drifting: ReconcileResult = {
  broker: "paper_sim",
  in_sync: false,
  checked: 2,
  drift: [{ symbol: "MSFT", ledger_qty: 10, broker_qty: 7, delta: -3 }],
};

export const positions: PositionRow[] = [
  {
    plan_id: "default",
    symbol: "MSFT",
    // the ledger stores the serialized params blob, not a bare name
    strategy: '{"period": 14, "type": "rsi"}',
    // positions are sized by cash, so quantities are routinely fractional
    qty: 23.7791,
    avg_price: 420.5,
    updated_at: "2026-08-01T20:30:00Z",
  },
];

export const orders: OrderRow[] = [
  {
    id: 1,
    created_at: "2026-08-01T20:30:00Z",
    updated_at: "2026-08-01T20:30:01Z",
    plan_id: "default",
    symbol: "MSFT",
    // the ledger stores the serialized params blob, not a bare name
    strategy: '{"period": 14, "type": "rsi"}',
    side: "buy",
    qty: 10,
    broker: "paper_sim",
    broker_order_id: "sim-1",
    status: "filled",
    filled_qty: 10,
    avg_fill_price: 420.5,
    reason: null,
    intent_id: 1,
  },
  {
    id: 2,
    created_at: "2026-08-01T20:31:00Z",
    updated_at: "2026-08-01T20:31:00Z",
    plan_id: "default",
    symbol: "NVDA",
    // the ledger stores the serialized params blob, not a bare name
    strategy: '{"period": 14, "type": "rsi"}',
    side: "buy",
    qty: 5,
    broker: "paper_sim",
    broker_order_id: "sim-2",
    status: "rejected",
    filled_qty: 0,
    avg_fill_price: 0,
    reason: "max_position_value exceeded",
    intent_id: 2,
  },
];

export const intents: IntentRow[] = [
  {
    id: 1,
    created_at: "2026-08-01T20:30:00Z",
    plan_id: "default",
    symbol: "MSFT",
    // the ledger stores the serialized params blob, not a bare name
    strategy: '{"period": 14, "type": "rsi"}',
    side: "buy",
    qty: 10,
    price: 420.5,
    signal: "buy",
    reason: "rsi crossed below 30",
    status: "filled",
    realized_pnl: null,
  },
  {
    id: 2,
    created_at: "2026-08-01T20:31:00Z",
    plan_id: "default",
    symbol: "NVDA",
    // the ledger stores the serialized params blob, not a bare name
    strategy: '{"period": 14, "type": "rsi"}',
    side: "buy",
    qty: 5,
    price: 900,
    signal: "buy",
    reason: "rsi crossed below 30",
    status: "rejected: max_position_value exceeded",
    realized_pnl: null,
  },
];

export const soak: SoakReport = {
  plan_id: "default",
  orders: 22,
  filled: 21,
  rejected: 1,
  fill_rate: 0.95,
  slippage_bps: {
    samples: 21,
    mean: 8,
    worst: 8,
    note: "positive = execution worse than the backtest assumed",
  },
  realized_pnl: 1420.56,
};

export const plans: TradePlan[] = [
  {
    plan_id: "default",
    created_at: "2026-08-01T10:00:00Z",
    updated_at: "2026-08-01T10:00:00Z",
    enabled: true,
    items: [
      {
        dataset: "MSFT.csv",
        strategy: { type: "rsi", period: 14 },
        cash_allocation: 10_000,
      },
    ],
  },
];

/** The full route table the trading console fetches on mount. */
export function consoleRoutes(overrides: Record<string, unknown> = {}) {
  return {
    "/trade/plans": { plans },
    "/trade/broker": paperBroker,
    "/trade/schedule": schedule,
    "/trade/killswitch": killSwitchOff,
    "/trade/limits": limitsForPlan,
    "/trade/positions": { positions },
    "/trade/orders": { orders },
    "/trade/intents": { intents },
    "/trade/soak": soak,
    "/trade/reconcile": inSync,
    ...overrides,
  };
}
