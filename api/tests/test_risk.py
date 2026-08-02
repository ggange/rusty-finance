"""Tests for pre-trade risk guardrails and the manual kill switch.

Covers the pure decision logic in api/risk.py and its enforcement through the
live tick path — a guardrail that isn't wired into the chokepoint is worthless,
so both layers are tested.
"""

import csv
from datetime import date, timedelta
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from api.risk import Decision, evaluate, resolve_limits


# ─── Fixtures ─────────────────────────────────────────────────────────────────

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
SELL_CANDLES = make_candles([100.0] * 30 + [200.0])

RSI2 = {"type": "rsi", "period": 2}


@pytest.fixture()
def data_dir(tmp_path):
    d = tmp_path / "datasets"
    d.mkdir()
    write_csv(d / "BUY.csv", BUY_CANDLES)
    write_csv(d / "SELL.csv", SELL_CANDLES)
    return d


@pytest.fixture()
def client(monkeypatch, tmp_path, data_dir):
    monkeypatch.setenv("RUSTY_FINANCE_DB", str(tmp_path / "test.db"))
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(data_dir))
    monkeypatch.setenv("RUSTY_FINANCE_SCHEDULER", "0")
    from api.main import app
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


def tick_body(dataset="BUY.csv", cash=10_000.0, plan_id="default"):
    return {
        "plan_id": plan_id,
        "items": [{"dataset": dataset, "strategy": RSI2, "cash_allocation": cash}],
    }


NO_LIMITS = {"max_position_value": None, "max_daily_loss": None, "max_daily_orders": None}
NO_STATS = {"orders": 0, "realized_pnl": 0.0}


# ─── resolve_limits ───────────────────────────────────────────────────────────

def test_resolve_limits_uses_global_when_plan_has_none():
    resolved = resolve_limits(None, {"max_position_value": 500.0})
    assert resolved["max_position_value"] == 500.0


def test_resolve_limits_plan_overrides_global():
    resolved = resolve_limits({"max_position_value": 100.0}, {"max_position_value": 500.0})
    assert resolved["max_position_value"] == 100.0


def test_resolve_limits_merges_field_by_field():
    """A plan setting one field still inherits the others from global."""
    resolved = resolve_limits(
        {"max_daily_loss": 50.0},
        {"max_position_value": 500.0, "max_daily_orders": 3},
    )
    assert resolved == {
        "max_position_value": 500.0,
        "max_daily_loss": 50.0,
        "max_daily_orders": 3,
    }


def test_resolve_limits_all_absent_is_unlimited():
    assert resolve_limits(None, None) == NO_LIMITS


# ─── evaluate: position sizing ────────────────────────────────────────────────

def test_buy_within_position_limit_allowed():
    limits = {**NO_LIMITS, "max_position_value": 1000.0}
    assert evaluate("buy", 5.0, 100.0, limits, NO_STATS).allowed


def test_buy_over_position_limit_rejected():
    limits = {**NO_LIMITS, "max_position_value": 1000.0}
    d = evaluate("buy", 20.0, 100.0, limits, NO_STATS)
    assert not d.allowed
    assert "max_position_value" in d.reason


def test_buy_exactly_at_position_limit_allowed():
    limits = {**NO_LIMITS, "max_position_value": 1000.0}
    assert evaluate("buy", 10.0, 100.0, limits, NO_STATS).allowed


# ─── evaluate: daily order cap ────────────────────────────────────────────────

def test_buy_under_daily_order_cap_allowed():
    limits = {**NO_LIMITS, "max_daily_orders": 3}
    assert evaluate("buy", 1.0, 10.0, limits, {"orders": 2, "realized_pnl": 0.0}).allowed


def test_buy_at_daily_order_cap_rejected():
    limits = {**NO_LIMITS, "max_daily_orders": 3}
    d = evaluate("buy", 1.0, 10.0, limits, {"orders": 3, "realized_pnl": 0.0})
    assert not d.allowed
    assert "max_daily_orders" in d.reason


# ─── evaluate: daily loss cap ─────────────────────────────────────────────────

def test_buy_under_daily_loss_cap_allowed():
    limits = {**NO_LIMITS, "max_daily_loss": 100.0}
    assert evaluate("buy", 1.0, 10.0, limits, {"orders": 0, "realized_pnl": -50.0}).allowed


def test_buy_at_daily_loss_cap_rejected():
    limits = {**NO_LIMITS, "max_daily_loss": 100.0}
    d = evaluate("buy", 1.0, 10.0, limits, {"orders": 0, "realized_pnl": -100.0})
    assert not d.allowed
    assert "max_daily_loss" in d.reason


def test_daily_loss_cap_sign_insensitive():
    """A limit given as -100 must behave the same as 100."""
    limits = {**NO_LIMITS, "max_daily_loss": -100.0}
    assert not evaluate("buy", 1.0, 10.0, limits, {"orders": 0, "realized_pnl": -150.0}).allowed


def test_profit_never_trips_loss_cap():
    limits = {**NO_LIMITS, "max_daily_loss": 100.0}
    assert evaluate("buy", 1.0, 10.0, limits, {"orders": 0, "realized_pnl": 500.0}).allowed


# ─── evaluate: exits are never blocked by limits ──────────────────────────────

@pytest.mark.parametrize("limits", [
    {**NO_LIMITS, "max_position_value": 1.0},
    {**NO_LIMITS, "max_daily_orders": 1},
    {**NO_LIMITS, "max_daily_loss": 1.0},
])
def test_sell_always_passes_limits(limits):
    """Guardrails constrain entries; blocking an exit would strand capital."""
    stats = {"orders": 99, "realized_pnl": -9999.0}
    assert evaluate("sell", 1000.0, 100.0, limits, stats).allowed


# ─── evaluate: kill switch ────────────────────────────────────────────────────

def test_kill_switch_blocks_buy():
    d = evaluate("buy", 1.0, 10.0, NO_LIMITS, NO_STATS, {"engaged": True, "reason": None})
    assert not d.allowed
    assert "kill switch" in d.reason


def test_kill_switch_blocks_sell_too():
    """Unlike a limit, a halt must stop exits as well — otherwise it isn't a halt."""
    d = evaluate("sell", 1.0, 10.0, NO_LIMITS, NO_STATS, {"engaged": True, "reason": None})
    assert not d.allowed


def test_kill_switch_reason_is_surfaced():
    d = evaluate("buy", 1.0, 10.0, NO_LIMITS, NO_STATS,
                 {"engaged": True, "reason": "broker outage"})
    assert "broker outage" in d.reason


def test_disengaged_kill_switch_allows():
    assert evaluate("buy", 1.0, 10.0, NO_LIMITS, NO_STATS, {"engaged": False}).allowed


def test_kill_switch_takes_precedence_over_limits():
    limits = {**NO_LIMITS, "max_position_value": 1.0}
    d = evaluate("buy", 1000.0, 100.0, limits, NO_STATS, {"engaged": True})
    assert "kill switch" in d.reason


def test_decision_status_strings():
    assert Decision(True).status == "allowed"
    assert Decision(False, "nope").status == "rejected: nope"


# ─── Limits API ───────────────────────────────────────────────────────────────

def test_set_and_get_global_limits(client):
    r = client.post("/trade/limits", json={"max_position_value": 5000.0})
    assert r.status_code == 200
    assert r.json()["plan_id"] == "__global__"

    body = client.get("/trade/limits").json()["limits"]
    assert len(body) == 1
    assert body[0]["max_position_value"] == 5000.0


def test_effective_limits_layer_plan_over_global(client):
    client.post("/trade/limits", json={"max_position_value": 5000.0, "max_daily_orders": 10})
    client.post("/trade/limits", json={"plan_id": "p1", "max_position_value": 100.0})

    eff = client.get("/trade/limits?plan_id=p1").json()["effective"]
    assert eff["max_position_value"] == 100.0
    assert eff["max_daily_orders"] == 10


def test_limits_reject_non_positive_values(client):
    assert client.post("/trade/limits", json={"max_position_value": 0}).status_code == 422
    assert client.post("/trade/limits", json={"max_daily_orders": -1}).status_code == 422


def test_delete_limits(client):
    client.post("/trade/limits", json={"plan_id": "p1", "max_position_value": 100.0})
    assert client.delete("/trade/limits/p1").status_code == 200
    assert client.delete("/trade/limits/p1").status_code == 404


# ─── Kill switch API ──────────────────────────────────────────────────────────

def test_kill_switch_defaults_to_disengaged(client):
    body = client.get("/trade/killswitch").json()
    assert body["engaged"] is False


def test_kill_switch_engage_and_release(client):
    engaged = client.post("/trade/killswitch",
                          json={"engaged": True, "reason": "manual halt"}).json()
    assert engaged["engaged"] is True
    assert engaged["reason"] == "manual halt"
    assert client.get("/trade/killswitch").json()["engaged"] is True

    released = client.post("/trade/killswitch", json={"engaged": False}).json()
    assert released["engaged"] is False


def test_kill_switch_survives_restart(monkeypatch, tmp_path, data_dir):
    """A halted system must stay halted across an API restart."""
    monkeypatch.setenv("RUSTY_FINANCE_DB", str(tmp_path / "persist.db"))
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(data_dir))
    monkeypatch.setenv("RUSTY_FINANCE_SCHEDULER", "0")
    from api.main import app

    with TestClient(app) as c:
        c.post("/trade/killswitch", json={"engaged": True, "reason": "overnight"})

    with TestClient(app) as c:  # fresh lifespan, same database
        body = c.get("/trade/killswitch").json()
        assert body["engaged"] is True
        assert body["reason"] == "overnight"


# ─── Enforcement through the live tick path ───────────────────────────────────

def test_tick_blocked_by_kill_switch(client):
    client.post("/trade/killswitch", json={"engaged": True, "reason": "halt"})
    body = client.post("/trade/tick", json=tick_body()).json()

    intent = body["results"][0]["intent"]
    assert intent["allowed"] is False
    assert "kill switch" in intent["rejected_reason"]
    assert body["positions"] == [], "a blocked order must not move the ledger"


def test_blocked_order_is_still_logged(client):
    client.post("/trade/killswitch", json={"engaged": True})
    client.post("/trade/tick", json=tick_body())

    intents = client.get("/trade/intents").json()["intents"]
    assert len(intents) == 1
    assert intents[0]["status"].startswith("rejected")


def test_tick_blocked_by_position_limit(client):
    # cash_allocation 10_000 at close 50 → 10_000 notional, over the 1_000 cap
    client.post("/trade/limits", json={"max_position_value": 1000.0})
    body = client.post("/trade/tick", json=tick_body()).json()

    intent = body["results"][0]["intent"]
    assert intent["allowed"] is False
    assert "max_position_value" in intent["rejected_reason"]
    assert body["positions"] == []


def test_tick_allowed_when_within_limits(client):
    client.post("/trade/limits", json={"max_position_value": 50_000.0})
    body = client.post("/trade/tick", json=tick_body()).json()

    assert body["results"][0]["intent"]["allowed"] is True
    assert len(body["positions"]) == 1


def test_releasing_kill_switch_restores_trading(client):
    client.post("/trade/killswitch", json={"engaged": True})
    blocked = client.post("/trade/tick", json=tick_body()).json()
    assert blocked["results"][0]["intent"]["allowed"] is False

    client.post("/trade/killswitch", json={"engaged": False})
    allowed = client.post("/trade/tick", json=tick_body()).json()
    assert allowed["results"][0]["intent"]["allowed"] is True
    assert len(allowed["positions"]) == 1


def test_rejected_order_does_not_consume_daily_budget(client):
    """Rejections must not eat the order cap, or one block would cascade."""
    client.post("/trade/limits", json={"max_position_value": 1.0, "max_daily_orders": 1})
    client.post("/trade/tick", json=tick_body())  # rejected on size

    # Raise the size cap; the order budget should still be intact
    client.post("/trade/limits", json={"max_position_value": 50_000.0, "max_daily_orders": 1})
    body = client.post("/trade/tick", json=tick_body()).json()
    assert body["results"][0]["intent"]["allowed"] is True


def test_exit_records_realized_pnl(client, data_dir):
    """Full round trip: enter at 50, then exit the same position at 200."""
    client.post("/trade/tick", json=tick_body(dataset="BUY.csv"))
    positions = client.get("/trade/positions").json()["positions"]
    assert positions[0]["qty"] == pytest.approx(200.0)  # 10_000 / 50
    assert positions[0]["avg_price"] == pytest.approx(50.0)

    # Same symbol and strategy, now with a bar that triggers the exit
    write_csv(data_dir / "BUY.csv", SELL_CANDLES)
    body = client.post("/trade/tick", json=tick_body(dataset="BUY.csv")).json()

    intent = body["results"][0]["intent"]
    assert intent["side"] == "sell"
    assert intent["allowed"] is True
    # (200 exit - 50 entry) * 200 shares
    assert intent["realized_pnl"] == pytest.approx(30_000.0)

    assert client.get("/trade/positions").json()["positions"][0]["qty"] == 0.0


def test_losing_exit_trips_daily_loss_cap(client, data_dir):
    """A realized loss must block further entries once it reaches the cap."""
    write_csv(data_dir / "BUY.csv", make_candles([100.0] * 30 + [50.0]))
    client.post("/trade/tick", json=tick_body(dataset="BUY.csv", cash=1_000.0))

    # Exit far below the 50.0 entry → a large realized loss
    write_csv(data_dir / "BUY.csv", make_candles([10.0] * 30 + [40.0]))
    exit_body = client.post("/trade/tick", json=tick_body(dataset="BUY.csv", cash=1_000.0)).json()
    assert exit_body["results"][0]["intent"]["realized_pnl"] < 0

    # Now cap the daily loss below what was just realized, and try to re-enter
    client.post("/trade/limits", json={"max_daily_loss": 1.0})
    write_csv(data_dir / "BUY.csv", make_candles([100.0] * 30 + [50.0]))
    reentry = client.post("/trade/tick", json=tick_body(dataset="BUY.csv", cash=1_000.0)).json()

    intent = reentry["results"][0]["intent"]
    assert intent["allowed"] is False
    assert "max_daily_loss" in intent["rejected_reason"]


def test_tick_response_reports_active_controls(client):
    client.post("/trade/limits", json={"max_position_value": 50_000.0})
    body = client.post("/trade/tick", json=tick_body()).json()

    assert body["limits"]["max_position_value"] == 50_000.0
    assert body["kill_switch"]["engaged"] is False


def test_scheduled_run_respects_kill_switch(client):
    client.post("/trade/plans", json=tick_body(plan_id="p1"))
    client.post("/trade/killswitch", json={"engaged": True})

    body = client.post("/trade/schedule/run?refresh=false").json()
    assert body["plans_run"] == 1
    intent = body["results"][0]["results"][0]["intent"]
    assert intent["allowed"] is False


# ─── Schema migration ─────────────────────────────────────────────────────────

def test_migration_adds_realized_pnl_to_preexisting_db(tmp_path, monkeypatch):
    """A database created before realized_pnl existed must upgrade in place.

    CREATE TABLE IF NOT EXISTS is a no-op on an existing table, so the column
    is added by an explicit migration — and old rows must survive it.
    """
    import asyncio
    import sqlite3

    import api.db as db

    path = tmp_path / "legacy.db"
    conn = sqlite3.connect(path)
    conn.executescript(
        """
        CREATE TABLE order_intents (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at TEXT NOT NULL, plan_id TEXT NOT NULL,
            symbol     TEXT NOT NULL, strategy TEXT NOT NULL,
            side       TEXT NOT NULL, qty REAL NOT NULL, price REAL NOT NULL,
            signal     TEXT NOT NULL, reason TEXT NOT NULL, status TEXT NOT NULL
        );
        INSERT INTO order_intents
          (created_at, plan_id, symbol, strategy, side, qty, price, signal, reason, status)
        VALUES ('2026-01-01T00:00:00+00:00','p','AAPL','s','buy',1.0,10.0,'buy','entry','dry_run');
        """
    )
    conn.commit()
    conn.close()

    monkeypatch.setenv("RUSTY_FINANCE_DB", str(path))
    asyncio.run(db.init_db())
    asyncio.run(db.init_db())  # must be idempotent

    conn = sqlite3.connect(path)
    cols = [r[1] for r in conn.execute("PRAGMA table_info(order_intents)")]
    rows = conn.execute("SELECT symbol, realized_pnl FROM order_intents").fetchall()
    tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    conn.close()

    assert "realized_pnl" in cols
    assert rows == [("AAPL", None)], "pre-existing rows must survive the migration"
    assert {"risk_limits", "kill_switch"} <= tables
