import { describe, expect, it, vi } from "vitest";
import { api } from "./apiClient";
import { errorResponse, mockFetch } from "../test/mockApi";

describe("apiClient error decoding", () => {
  it("joins pydantic 422 validation issues into one message", async () => {
    mockFetch({
      "/portfolio": errorResponse(422, {
        detail: [
          { loc: ["body", "assets"], msg: "field required", type: "missing" },
          { loc: ["body", "initial_cash"], msg: "must be > 0", type: "value_error" },
        ],
      }),
    });

    await expect(api.portfolio({} as never)).rejects.toThrow(
      "assets: field required; initial_cash: must be > 0",
    );
  });

  it("passes a string detail through verbatim, e.g. the 503 engine message", async () => {
    mockFetch({
      "/portfolio": errorResponse(503, { detail: "backtesting_py not installed" }),
    });

    await expect(api.portfolio({} as never)).rejects.toThrow("backtesting_py not installed");
  });

  it("falls back to the status code when the body is not JSON", async () => {
    mockFetch({ "/health": new Response("<html>502</html>", { status: 502 }) });

    await expect(api.health()).rejects.toThrow("Request failed (HTTP 502)");
  });

  it("reports an unreachable backend rather than a raw network error", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("Failed to fetch");
      }),
    );

    await expect(api.health()).rejects.toThrow(/backend running on :8000/);
  });
});

describe("apiClient trading routes", () => {
  it("omits undefined query params instead of sending 'undefined'", async () => {
    const fetchMock = mockFetch({ "/trade/orders": { orders: [] } });

    await api.trade.orders({ planId: "default" });

    const url = String(fetchMock.mock.calls[0][0]);
    expect(url).toContain("plan_id=default");
    expect(url).not.toContain("open_only");
    expect(url).not.toContain("undefined");
  });

  it("url-encodes plan ids in DELETE paths", async () => {
    const fetchMock = mockFetch({ "/trade/plans": { deleted: "a b" } });

    await api.trade.deletePlan("a b");

    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain("/trade/plans/a%20b");
    expect((init as RequestInit).method).toBe("DELETE");
  });

  it("posts the kill switch with an explicit null reason when none is given", async () => {
    const fetchMock = mockFetch({
      "/trade/killswitch": { engaged: true, reason: null, updated_at: null },
    });

    await api.trade.setKillSwitch(true);

    const init = fetchMock.mock.calls[0][1] as RequestInit;
    expect(JSON.parse(init.body as string)).toEqual({ engaged: true, reason: null });
  });
});
