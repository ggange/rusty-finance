"""Tests for stored trading plans and the wall-clock scheduler.

Network access is always mocked — no test here hits Yahoo.
"""

import csv
from datetime import date, timedelta
from pathlib import Path

import pytest
from fastapi.testclient import TestClient


# ─── Fixtures (mirrors test_trading.py's dataset setup) ───────────────────────

def make_candles(closes: list[float]) -> list[dict]:
    base = date(2024, 1, 1)
    return [
        {
            "date": (base + timedelta(days=i)).isoformat(),
            "open": c, "high": c, "low": c, "close": c, "volume": 1000,
        }
        for i, c in enumerate(closes)
    ]


def write_csv(path: Path, candles: list[dict]) -> None:
    with path.open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["Date", "Open", "High", "Low", "Close", "Volume"])
        w.writeheader()
        for c in candles:
            w.writerow({
                "Date": c["date"], "Open": c["open"], "High": c["high"],
                "Low": c["low"], "Close": c["close"], "Volume": c["volume"],
            })


BUY_CANDLES = make_candles([100.0] * 30 + [50.0])
HOLD_CANDLES = make_candles([100.0] * 30)

RSI2 = {"type": "rsi", "period": 2}


@pytest.fixture()
def data_dir(tmp_path):
    d = tmp_path / "datasets"
    d.mkdir()
    write_csv(d / "BUY.csv", BUY_CANDLES)
    write_csv(d / "HOLD.csv", HOLD_CANDLES)
    return d


@pytest.fixture()
def client(monkeypatch, tmp_path, data_dir):
    monkeypatch.setenv("RUSTY_FINANCE_DB", str(tmp_path / "test.db"))
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(data_dir))
    monkeypatch.setenv("RUSTY_FINANCE_SCHEDULER", "0")
    from api.main import app
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


@pytest.fixture()
def no_network(monkeypatch):
    """Make the scheduler's data refresh a no-op that records its calls."""
    from api import scheduler

    calls = []

    async def fake_refresh(symbols):
        calls.append(list(symbols))
        return [{"ticker": s, "status": "no_data", "added": 0} for s in symbols]

    monkeypatch.setattr(scheduler, "refresh_symbols", fake_refresh)
    return calls


def plan_body(plan_id="default", dataset="BUY.csv", enabled=True):
    return {
        "plan_id": plan_id,
        "enabled": enabled,
        "items": [{"dataset": dataset, "strategy": RSI2, "cash_allocation": 10_000.0}],
    }


# ─── Plan CRUD ────────────────────────────────────────────────────────────────

def test_create_plan_returns_stored_plan(client):
    r = client.post("/trade/plans", json=plan_body())
    assert r.status_code == 200
    body = r.json()
    assert body["plan_id"] == "default"
    assert body["enabled"] is True
    assert len(body["items"]) == 1
    assert body["items"][0]["dataset"] == "BUY.csv"


def test_create_plan_rejects_unknown_dataset(client):
    r = client.post("/trade/plans", json=plan_body(dataset="NOPE.csv"))
    assert r.status_code == 404


def test_create_plan_is_idempotent_upsert(client):
    client.post("/trade/plans", json=plan_body())
    client.post("/trade/plans", json=plan_body(dataset="HOLD.csv"))

    plans = client.get("/trade/plans").json()["plans"]
    assert len(plans) == 1
    assert plans[0]["items"][0]["dataset"] == "HOLD.csv"


def test_list_plans_enabled_only_filters(client):
    client.post("/trade/plans", json=plan_body(plan_id="on", enabled=True))
    client.post("/trade/plans", json=plan_body(plan_id="off", enabled=False))

    all_plans = client.get("/trade/plans").json()["plans"]
    enabled = client.get("/trade/plans?enabled_only=true").json()["plans"]

    assert len(all_plans) == 2
    assert [p["plan_id"] for p in enabled] == ["on"]


def test_delete_plan(client):
    client.post("/trade/plans", json=plan_body())
    assert client.delete("/trade/plans/default").status_code == 200
    assert client.get("/trade/plans").json()["plans"] == []


def test_delete_missing_plan_404s(client):
    assert client.delete("/trade/plans/ghost").status_code == 404


# ─── /trade/tick falling back to the stored plan ──────────────────────────────

def test_tick_without_items_uses_stored_plan(client):
    client.post("/trade/plans", json=plan_body())
    r = client.post("/trade/tick", json={"plan_id": "default"})

    assert r.status_code == 200
    body = r.json()
    assert body["results"][0]["symbol"] == "BUY"
    assert body["results"][0]["intent"]["side"] == "buy"


def test_tick_without_items_and_no_plan_404s(client):
    r = client.post("/trade/tick", json={"plan_id": "nothing-here"})
    assert r.status_code == 404


def test_tick_with_explicit_items_still_works(client):
    r = client.post("/trade/tick", json=plan_body())
    assert r.status_code == 200
    assert r.json()["results"][0]["intent"]["side"] == "buy"


# ─── Scheduler status ─────────────────────────────────────────────────────────

def test_schedule_status_reports_disabled(client):
    body = client.get("/trade/schedule").json()
    assert body["enabled"] is False
    assert body["running"] is False
    assert body["last_run"] is None


def test_schedule_status_exposes_cron_config(client):
    cron = client.get("/trade/schedule").json()["cron"]
    assert cron["day_of_week"] == "mon-fri"
    assert cron["hour"] == 16
    assert cron["minute"] == 30
    assert cron["timezone"] == "America/New_York"


def test_cron_config_honours_env_overrides(monkeypatch):
    from api import scheduler

    monkeypatch.setenv("RUSTY_FINANCE_SCHEDULER_HOUR", "9")
    monkeypatch.setenv("RUSTY_FINANCE_SCHEDULER_MINUTE", "5")
    monkeypatch.setenv("RUSTY_FINANCE_SCHEDULER_TZ", "Europe/London")

    cfg = scheduler.cron_config()
    assert (cfg["hour"], cfg["minute"], cfg["timezone"]) == (9, 5, "Europe/London")


def test_started_scheduler_reports_next_run(monkeypatch, tmp_path, data_dir):
    """With the scheduler enabled, status exposes a real next fire time."""
    monkeypatch.setenv("RUSTY_FINANCE_DB", str(tmp_path / "sched.db"))
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(data_dir))
    monkeypatch.setenv("RUSTY_FINANCE_SCHEDULER", "1")

    from api.main import app
    with TestClient(app, raise_server_exceptions=False) as c:
        body = c.get("/trade/schedule").json()
        assert body["enabled"] is True
        assert body["running"] is True
        assert body["next_run"] is not None


# ─── run_all_plans ────────────────────────────────────────────────────────────

def test_manual_run_ticks_enabled_plans(client, no_network):
    client.post("/trade/plans", json=plan_body(plan_id="p1"))
    r = client.post("/trade/schedule/run")

    assert r.status_code == 200
    body = r.json()
    assert body["plans_run"] == 1
    assert body["plans_failed"] == 0
    assert body["intents_emitted"] == 1


def test_manual_run_skips_disabled_plans(client, no_network):
    client.post("/trade/plans", json=plan_body(plan_id="off", enabled=False))
    body = client.post("/trade/schedule/run").json()

    assert body["plans_run"] == 0
    assert body["intents_emitted"] == 0


def test_manual_run_refreshes_each_plan_symbol_once(client, no_network):
    client.post("/trade/plans", json=plan_body(plan_id="a", dataset="BUY.csv"))
    client.post("/trade/plans", json=plan_body(plan_id="b", dataset="BUY.csv"))
    client.post("/trade/schedule/run")

    # BUY appears in two plans but must only be fetched once
    assert no_network == [["BUY"]]


def test_manual_run_with_refresh_false_skips_fetch(client, no_network):
    client.post("/trade/plans", json=plan_body())
    client.post("/trade/schedule/run?refresh=false")
    assert no_network == []


def test_second_run_is_idempotent(client, no_network):
    """Re-running while already long must not emit a duplicate buy."""
    client.post("/trade/plans", json=plan_body())

    first = client.post("/trade/schedule/run").json()
    second = client.post("/trade/schedule/run").json()

    assert first["intents_emitted"] == 1
    assert second["intents_emitted"] == 0


def test_run_is_recorded_as_last_run(client, no_network):
    client.post("/trade/plans", json=plan_body())
    client.post("/trade/schedule/run")

    last = client.get("/trade/schedule").json()["last_run"]
    assert last is not None
    assert last["kind"] == "scheduled_tick"
    assert last["result"]["plans_run"] == 1


def test_one_failing_plan_does_not_abort_the_others(client, no_network, monkeypatch):
    from api import trading

    client.post("/trade/plans", json=plan_body(plan_id="good", dataset="BUY.csv"))
    client.post("/trade/plans", json=plan_body(plan_id="bad", dataset="BUY.csv"))

    real_resolve = trading.resolve_items

    def flaky(items):
        if flaky.calls == 0:  # first plan alphabetically is "bad"
            flaky.calls += 1
            raise RuntimeError("boom")
        flaky.calls += 1
        return real_resolve(items)

    flaky.calls = 0
    monkeypatch.setattr(trading, "resolve_items", flaky)

    body = client.post("/trade/schedule/run").json()

    assert body["plans_run"] == 2
    assert body["plans_failed"] == 1
    statuses = {p["plan_id"]: p["status"] for p in body["results"]}
    assert statuses == {"bad": "error", "good": "ok"}


def test_run_with_no_plans_is_harmless(client, no_network):
    body = client.post("/trade/schedule/run").json()
    assert body["plans_run"] == 0
    assert body["refreshed"] == []


# ─── plan_symbols helper ──────────────────────────────────────────────────────

def test_plan_symbols_dedupes_and_strips_extension():
    from api.scheduler import plan_symbols

    plans = [
        {"items": [{"dataset": "AAPL.csv"}, {"dataset": "MSFT.csv"}]},
        {"items": [{"dataset": "AAPL.csv"}]},
    ]
    assert plan_symbols(plans) == ["AAPL", "MSFT"]


def test_plan_symbols_handles_empty_plans():
    from api.scheduler import plan_symbols

    assert plan_symbols([]) == []
    assert plan_symbols([{"items": []}]) == []
