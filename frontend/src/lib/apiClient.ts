import type {
  ApiErrorBody,
  BacktestRequest,
  BacktestResponse,
  DatasetCandlesResponse,
  DatasetsResponse,
  HealthResponse,
  PortfolioRequest,
  PortfolioResponse,
  StrategiesResponse,
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

export const api = {
  health: () => request<HealthResponse>("/health"),
  strategies: () => request<StrategiesResponse>("/strategies"),
  datasets: () => request<DatasetsResponse>("/datasets"),
  dataset: (name: string) =>
    request<DatasetCandlesResponse>(`/datasets/${encodeURIComponent(name)}`),
  backtest: (body: BacktestRequest) =>
    request<BacktestResponse>("/backtest", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  portfolio: (body: PortfolioRequest) =>
    request<PortfolioResponse>("/portfolio", {
      method: "POST",
      body: JSON.stringify(body),
    }),
};
