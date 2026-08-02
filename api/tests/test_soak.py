"""Persistent paper venue, broker selection, and soak reporting.

The soak runs for weeks, so the venue's books must survive a restart — a
process-memory broker would report an empty venue after a bounce and make
reconciliation meaningless.
"""

import asyncio
import csv
import json
from datetime import date, timedelta
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

import api.db as db
from api import trading
from api.broker import (
    DryRunBroker,
    OrderIntent,
    PersistentPaperBroker,
    broker_config,
    make_broker,
)


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
RSI2 = {"type": "rsi", "period": 2}
RSI2_KEY = json.dumps({"period": 2, "type": "rsi"}, sort_keys=True)


@pytest.fixture()
def data_dir(tmp_path):
    d = tmp_path / "datasets"
    d.mkdir()
    write_csv(d / "BUY.csv", BUY_CANDLES)
    return d


@pytest.fixture()
def db_path(monkeypatch, tmp_path, data_dir):
    monkeypatch.setenv("RUSTY_FINANCE_DB", str(tmp_path / "test.db"))
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(data_dir))
    monkeypatch.setenv("RUSTY_FINANCE_SCHEDULER", "0")
    return tmp_path / "test.db"


@pytest.fixture()
def client(db_path):
    from api.main import app
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


def items(cash=10_000.0):
    return trading.resolve_items(
        [{"dataset": "BUY.csv", "strategy": RSI2, "cash_allocation": cash}]
    )


def intent(symbol="X", side="buy", qty=10.0, price=100.0):
    return OrderIntent(symbol, side, qty, price, "r", "s")


# ─── Broker selection ─────────────────────────────────────────────────────────

def test_default_broker_is_dry_run(monkeypatch):
    monkeypatch.delenv("RUSTY_FINANCE_BROKER", raising=False)
    assert broker_config()["kind"] == "dry_run"
    assert isinstance(make_broker(), DryRunBroker)


def test_paper_sim_selected_by_env(monkeypatch):
    monkeypatch.setenv("RUSTY_FINANCE_BROKER", "paper_sim")
    assert isinstance(make_broker(), PersistentPaperBroker)


def test_unknown_broker_falls_back_to_dry_run(monkeypatch):
    """An unrecognised name must not silently become something dangerous."""
    monkeypatch.setenv("RUSTY_FINANCE_BROKER", "definitely-not-a-broker")
    assert broker_config()["kind"] == "dry_run"
    assert isinstance(make_broker(), DryRunBroker)


def test_broker_tuning_from_env(monkeypatch):
    monkeypatch.setenv("RUSTY_FINANCE_BROKER", "paper_sim")
    monkeypatch.setenv("RUSTY_FINANCE_BROKER_SLIPPAGE", "0.002")
    monkeypatch.setenv("RUSTY_FINANCE_BROKER_FILL_RATIO", "0.5")

    broker = make_broker()
    assert broker.slippage == 0.002
    assert broker.fill_ratio == 0.5


def test_malformed_numeric_env_falls_back(monkeypatch):
    monkeypatch.setenv("RUSTY_FINANCE_BROKER_SLIPPAGE", "not-a-number")
    assert broker_config()["slippage"] == 0.0


def test_no_configured_broker_is_live(monkeypatch):
    """Nothing shipped today may move money."""
    for kind in ("dry_run", "paper_sim"):
        monkeypatch.setenv("RUSTY_FINANCE_BROKER", kind)
        assert make_broker().is_live is False


def test_broker_endpoint_reports_config(client, monkeypatch):
    body = client.get("/trade/broker").json()
    assert body["kind"] == "dry_run"
    assert body["is_live"] is False


# ─── Persistent venue ─────────────────────────────────────────────────────────

def test_persistent_broker_records_position(db_path):
    b = PersistentPaperBroker()
    asyncio.run(b.submit(intent()))
    assert asyncio.run(b.list_positions()) == [
        {"symbol": "X", "qty": 10.0, "avg_price": 100.0}
    ]


def test_persistent_broker_survives_new_instance(db_path):
    """A restart is modelled by a fresh broker object against the same DB."""
    asyncio.run(PersistentPaperBroker().submit(intent()))

    revived = PersistentPaperBroker()
    assert asyncio.run(revived.list_positions())[0]["qty"] == 10.0


def test_persistent_order_is_retrievable_after_restart(db_path):
    order = asyncio.run(PersistentPaperBroker().submit(intent()))

    revived = asyncio.run(PersistentPaperBroker().get_order(order.broker_order_id))
    assert revived is not None
    assert revived.status == "filled"
    assert revived.filled_qty == 10.0


def test_persistent_order_ids_do_not_collide_after_restart(db_path):
    first = asyncio.run(PersistentPaperBroker().submit(intent()))
    second = asyncio.run(PersistentPaperBroker().submit(intent()))
    assert first.broker_order_id != second.broker_order_id


def test_persistent_broker_averages_across_buys(db_path):
    b = PersistentPaperBroker()
    asyncio.run(b.submit(intent(qty=10.0, price=100.0)))
    asyncio.run(b.submit(intent(qty=10.0, price=200.0)))

    pos = asyncio.run(b.list_positions())[0]
    assert pos["qty"] == 20.0
    assert pos["avg_price"] == pytest.approx(150.0)


def test_persistent_broker_sell_clears_position(db_path):
    b = PersistentPaperBroker()
    asyncio.run(b.submit(intent(qty=10.0)))
    asyncio.run(b.submit(intent(side="sell", qty=10.0)))
    assert asyncio.run(b.list_positions()) == []


def test_persistent_broker_partial_fill(db_path):
    b = PersistentPaperBroker(fill_ratio=0.25)
    order = asyncio.run(b.submit(intent(qty=100.0)))

    assert order.status == "partially_filled"
    assert asyncio.run(b.list_positions())[0]["qty"] == pytest.approx(25.0)


def test_persistent_rejection_leaves_venue_flat(db_path):
    b = PersistentPaperBroker(reject_symbols={"X"})
    order = asyncio.run(b.submit(intent()))

    assert order.status == "rejected"
    assert asyncio.run(b.list_positions()) == []


def test_reconcile_stays_in_sync_across_restart(db_path):
    """The point of persistence: a bounce must not manufacture phantom drift."""
    asyncio.run(trading.run_tick("p", items(), PersistentPaperBroker()))

    rec = asyncio.run(trading.reconcile("p", PersistentPaperBroker()))
    assert rec["in_sync"] is True


# ─── Soak report ──────────────────────────────────────────────────────────────

def test_soak_report_empty_is_safe(db_path):
    report = asyncio.run(trading.soak_report("p"))
    assert report["orders"] == 0
    assert report["fill_rate"] is None
    assert report["slippage_bps"]["mean"] is None


def test_soak_report_counts_fills(db_path):
    asyncio.run(trading.run_tick("p", items(), PersistentPaperBroker()))
    report = asyncio.run(trading.soak_report("p"))

    assert report["orders"] == 1
    assert report["filled"] == 1
    assert report["rejected"] == 0
    assert report["fill_rate"] == pytest.approx(1.0)


def test_soak_report_measures_adverse_slippage_on_buys(db_path):
    """A buy filled above the signal price is worse than the backtest assumed."""
    asyncio.run(trading.run_tick("p", items(), PersistentPaperBroker(slippage=0.001)))
    report = asyncio.run(trading.soak_report("p"))

    assert report["slippage_bps"]["samples"] == 1
    assert report["slippage_bps"]["mean"] == pytest.approx(10.0)  # 0.1% = 10bps


def test_soak_report_zero_slippage_is_zero_bps(db_path):
    asyncio.run(trading.run_tick("p", items(), PersistentPaperBroker(slippage=0.0)))
    assert asyncio.run(trading.soak_report("p"))["slippage_bps"]["mean"] == pytest.approx(0.0)


def test_soak_report_counts_rejections(db_path):
    broker = PersistentPaperBroker(reject_symbols={"BUY"})
    asyncio.run(trading.run_tick("p", items(), broker))
    report = asyncio.run(trading.soak_report("p"))

    assert report["rejected"] == 1
    assert report["filled"] == 0


def test_soak_report_fill_rate_reflects_partials(db_path):
    asyncio.run(trading.run_tick("p", items(), PersistentPaperBroker(fill_ratio=0.4)))
    assert asyncio.run(trading.soak_report("p"))["fill_rate"] == pytest.approx(0.4)


def test_soak_endpoint(client):
    client.post("/trade/tick", json={
        "plan_id": "p", "items": [{"dataset": "BUY.csv", "strategy": RSI2, "cash_allocation": 100}],
    })
    body = client.get("/trade/soak?plan_id=p").json()
    assert body["orders"] == 1
    assert body["filled"] == 1


def test_scheduled_run_uses_configured_broker(client, monkeypatch):
    monkeypatch.setenv("RUSTY_FINANCE_BROKER", "paper_sim")
    client.post("/trade/plans", json={
        "plan_id": "p", "items": [{"dataset": "BUY.csv", "strategy": RSI2, "cash_allocation": 100}],
    })
    client.post("/trade/schedule/run?refresh=false")

    orders = client.get("/trade/orders?plan_id=p").json()["orders"]
    assert orders[0]["broker"] == "paper_sim"


# ─── Engine availability ──────────────────────────────────────────────────────

def test_run_tick_without_engine_raises_legibly(db_path, monkeypatch):
    """The scheduler calls run_tick directly, bypassing the HTTP engine guard.

    Without an explicit check the missing binding surfaced as a bare
    "NameError: name 'bt' is not defined" inside a cron log.
    """
    monkeypatch.setattr(trading, "_ENGINE_AVAILABLE", False)

    with pytest.raises(trading.EngineUnavailable, match="maturin develop"):
        asyncio.run(trading.run_tick("p", items(), PersistentPaperBroker()))


def test_scheduled_run_reports_engine_failure_per_plan(client, monkeypatch):
    """A missing engine must be recorded as a plan failure, not crash the cycle."""
    client.post("/trade/plans", json={
        "plan_id": "p", "items": [{"dataset": "BUY.csv", "strategy": RSI2, "cash_allocation": 100}],
    })
    monkeypatch.setattr(trading, "_ENGINE_AVAILABLE", False)

    from api import scheduler
    body = asyncio.run(scheduler.run_all_plans(refresh=False))

    assert body["plans_failed"] == 1
    assert "maturin develop" in body["results"][0]["error"]
