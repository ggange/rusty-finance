import type {
  ApiErrorBody,
  BrokerInfo,
  DatasetCandlesResponse,
  DatasetsResponse,
  HealthResponse,
  IntentsResponse,
  KillSwitch,
  LimitsForPlan,
  LimitsListResponse,
  OrdersResponse,
  PortfolioRequest,
  PortfolioResponse,
  PositionsResponse,
  ReconcileResult,
  RiskLimitsRequest,
  RiskLimitsRow,
  RunDetail,
  RunsResponse,
  ScheduleCycle,
  ScheduleStatus,
  SoakReport,
  StrategiesResponse,
  SweepRequest,
  SweepResponse,
  TickResult,
  TradePlan,
  TradePlanRequest,
  TradePlansResponse,
  TradeTickRequest,
  WalkForwardRequest,
  WalkForwardResponse,
} from "../types/api";

// All requests go through the Vite proxy at /api, which strips the prefix and
// forwards to the FastAPI backend on :8000. No CORS needed in local dev.
const BASE = "/api";

async function toApiError(res: Response): Promise<Error> {
  let body: ApiErrorBody | null = null;
  try {
    body = (await res.json()) as ApiErrorBody;
  } catch {
    // non-JSON body; fall through to status-based message
  }

  const detail = body?.detail;
  if (Array.isArray(detail)) {
    // pydantic 422: join each validation issue as "field: message"
    const msg = detail
      .map((d) => {
        const field = d.loc.filter((p) => p !== "body").join(".");
        return field ? `${field}: ${d.msg}` : d.msg;
      })
      .join("; ");
    return new Error(msg || `Request failed (${res.status})`);
  }
  if (typeof detail === "string") {
    return new Error(detail); // e.g. 503 engine-unavailable message
  }
  return new Error(`Request failed (HTTP ${res.status})`);
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`${BASE}${path}`, {
      headers: { "Content-Type": "application/json" },
      ...init,
    });
  } catch {
    throw new Error(
      "Could not reach the API. Is the backend running on :8000?",
    );
  }
  if (!res.ok) throw await toApiError(res);
  return (await res.json()) as T;
}

function post<T>(path: string, body: unknown): Promise<T> {
  return request<T>(path, { method: "POST", body: JSON.stringify(body) });
}

function del<T>(path: string): Promise<T> {
  return request<T>(path, { method: "DELETE" });
}

/** Build a query string, dropping undefined/null params. */
function qs(params: Record<string, string | number | boolean | undefined | null>): string {
  const sp = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null) sp.set(k, String(v));
  }
  const s = sp.toString();
  return s ? `?${s}` : "";
}

export const api = {
  health: () => request<HealthResponse>("/health"),
  strategies: () => request<StrategiesResponse>("/strategies"),
  datasets: () => request<DatasetsResponse>("/datasets"),
  dataset: (name: string) =>
    request<DatasetCandlesResponse>(`/datasets/${encodeURIComponent(name)}`),
  portfolio: (body: PortfolioRequest) => post<PortfolioResponse>("/portfolio", body),
  sweep: (body: SweepRequest) => post<SweepResponse>("/sweep", body),
  runs: (limit = 50) => request<RunsResponse>(`/runs${qs({ limit })}`),
  run: (id: number) => request<RunDetail>(`/runs/${id}`),
  walkForward: (body: WalkForwardRequest) =>
    post<WalkForwardResponse>("/walkforward", body),

  // ─── Trading ──────────────────────────────────────────────────────────────
  // Everything under /trade/* drives the live loop. The two calls that can
  // actually place orders — tick and scheduleRun — are marked below; callers
  // are expected to confirm with the user first.

  trade: {
    plans: (enabledOnly = false) =>
      request<TradePlansResponse>(`/trade/plans${qs({ enabled_only: enabledOnly })}`),
    savePlan: (body: TradePlanRequest) => post<TradePlan>("/trade/plans", body),
    deletePlan: (planId: string) =>
      del<{ deleted: string }>(`/trade/plans/${encodeURIComponent(planId)}`),

    /** Places orders. */
    tick: (body: TradeTickRequest) => post<TickResult>("/trade/tick", body),

    schedule: () => request<ScheduleStatus>("/trade/schedule"),
    /** Refreshes bars, then places orders for every enabled plan. */
    scheduleRun: (refresh?: boolean) =>
      post<ScheduleCycle>(`/trade/schedule/run${qs({ refresh })}`, {}),

    limits: () => request<LimitsListResponse>("/trade/limits"),
    limitsForPlan: (planId: string) =>
      request<LimitsForPlan>(`/trade/limits${qs({ plan_id: planId })}`),
    setLimits: (body: RiskLimitsRequest) => post<RiskLimitsRow>("/trade/limits", body),
    deleteLimits: (planId: string) =>
      del<{ deleted: string }>(`/trade/limits/${encodeURIComponent(planId)}`),

    killSwitch: () => request<KillSwitch>("/trade/killswitch"),
    setKillSwitch: (engaged: boolean, reason?: string | null) =>
      post<KillSwitch>("/trade/killswitch", { engaged, reason: reason ?? null }),

    orders: (opts: { planId?: string; openOnly?: boolean; limit?: number } = {}) =>
      request<OrdersResponse>(
        `/trade/orders${qs({
          plan_id: opts.planId,
          open_only: opts.openOnly,
          limit: opts.limit,
        })}`,
      ),
    intents: (opts: { planId?: string; limit?: number } = {}) =>
      request<IntentsResponse>(
        `/trade/intents${qs({ plan_id: opts.planId, limit: opts.limit })}`,
      ),
    positions: (planId?: string) =>
      request<PositionsResponse>(`/trade/positions${qs({ plan_id: planId })}`),

    broker: () => request<BrokerInfo>("/trade/broker"),
    soak: (planId?: string) => request<SoakReport>(`/trade/soak${qs({ plan_id: planId })}`),
    reconcile: (planId = "default") =>
      request<ReconcileResult>(`/trade/reconcile${qs({ plan_id: planId })}`),
  },
};
