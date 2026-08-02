"""Order lifecycle, fail-closed risk, and position reconciliation.

The ledger must follow what a venue actually *filled*, not what we asked for —
these tests drive partial fills and rejections through the real tick path via
SimulatedPaperBroker.
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
from api.broker import DryRunBroker, OrderIntent, SimulatedPaperBroker, apply_fill
from api.risk import evaluate


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

NO_LIMITS = {"max_position_value": None, "max_daily_loss": None, "max_daily_orders": None}
NO_STATS = {"orders": 0, "realized_pnl": 0.0}


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


def items(dataset="BUY.csv", cash=10_000.0):
    return trading.resolve_items(
        [{"dataset": dataset, "strategy": RSI2, "cash_allocation": cash}]
    )


# ─── apply_fill helper ────────────────────────────────────────────────────────

def test_apply_fill_weights_average_price():
    positions = {}
    apply_fill(positions, "X", "buy", 10.0, 100.0)
    apply_fill(positions, "X", "buy", 10.0, 200.0)
    assert positions["X"]["qty"] == 20.0
    assert positions["X"]["avg_price"] == pytest.approx(150.0)


def test_apply_fill_sell_reduces_and_clears():
    positions = {}
    apply_fill(positions, "X", "buy", 10.0, 100.0)
    apply_fill(positions, "X", "sell", 10.0, 120.0)
    assert positions["X"]["qty"] == 0.0
    assert positions["X"]["avg_price"] == 0.0


def test_apply_fill_sell_never_goes_negative():
    positions = {}
    apply_fill(positions, "X", "buy", 5.0, 100.0)
    apply_fill(positions, "X", "sell", 50.0, 100.0)
    assert positions["X"]["qty"] == 0.0


# ─── SimulatedPaperBroker ─────────────────────────────────────────────────────

def test_simulated_broker_full_fill():
    b = SimulatedPaperBroker()
    intent = OrderIntent("X", "buy", 10.0, 100.0, "r", "s")
    order = asyncio.run(b.submit(intent))
    assert order.status == "filled"
    assert order.filled_qty == 10.0


def test_simulated_broker_partial_fill():
    b = SimulatedPaperBroker(fill_ratio=0.4)
    order = asyncio.run(b.submit(OrderIntent("X", "buy", 10.0, 100.0, "r", "s")))
    assert order.status == "partially_filled"
    assert order.filled_qty == pytest.approx(4.0)
    assert not order.is_terminal


def test_simulated_broker_rejection():
    b = SimulatedPaperBroker(reject_symbols={"X"})
    order = asyncio.run(b.submit(OrderIntent("X", "buy", 10.0, 100.0, "r", "s")))
    assert order.status == "rejected"
    assert order.filled_qty == 0.0
    assert order.is_terminal


def test_simulated_broker_slippage_is_always_adverse():
    b = SimulatedPaperBroker(slippage=0.01)
    buy = asyncio.run(b.submit(OrderIntent("X", "buy", 1.0, 100.0, "r", "s")))
    sell = asyncio.run(b.submit(OrderIntent("Y", "sell", 1.0, 100.0, "r", "s")))
    assert buy.avg_fill_price > 100.0
    assert sell.avg_fill_price < 100.0


def test_simulated_broker_tracks_positions():
    b = SimulatedPaperBroker()
    asyncio.run(b.submit(OrderIntent("X", "buy", 10.0, 100.0, "r", "s")))
    assert asyncio.run(b.list_positions()) == [{"symbol": "X", "qty": 10.0, "avg_price": 100.0}]


def test_rejected_order_does_not_move_broker_positions():
    b = SimulatedPaperBroker(reject_symbols={"X"})
    asyncio.run(b.submit(OrderIntent("X", "buy", 10.0, 100.0, "r", "s")))
    assert asyncio.run(b.list_positions()) == []


def test_get_order_returns_none_for_unknown_id():
    assert asyncio.run(SimulatedPaperBroker().get_order("nope")) is None


# ─── Fail-closed risk ─────────────────────────────────────────────────────────

def test_live_broker_without_limits_is_refused():
    d = evaluate("buy", 1.0, 10.0, NO_LIMITS, NO_STATS, require_limits=True)
    assert not d.allowed
    assert "max_position_value" in d.reason
    assert "max_daily_loss" in d.reason


def test_live_broker_with_partial_limits_is_refused():
    limits = {**NO_LIMITS, "max_position_value": 1000.0}
    d = evaluate("buy", 1.0, 10.0, limits, NO_STATS, require_limits=True)
    assert not d.allowed
    assert "max_daily_loss" in d.reason


def test_live_broker_with_required_limits_is_allowed():
    limits = {"max_position_value": 1000.0, "max_daily_loss": 100.0, "max_daily_orders": None}
    assert evaluate("buy", 1.0, 10.0, limits, NO_STATS, require_limits=True).allowed


def test_fail_closed_blocks_exits_too():
    """An unconfigured live system shouldn't be trading in either direction."""
    d = evaluate("sell", 1.0, 10.0, NO_LIMITS, NO_STATS, require_limits=True)
    assert not d.allowed


def test_dry_run_stays_permissive_without_limits():
    assert evaluate("buy", 1.0, 10.0, NO_LIMITS, NO_STATS, require_limits=False).allowed


def test_live_tick_is_refused_without_limits(db_path):
    """End-to-end: a broker claiming is_live cannot trade unconfigured."""
    class Liveish(DryRunBroker):
        name = "liveish"
        is_live = True

    body = asyncio.run(trading.run_tick("p", items(), Liveish()))
    intent = body["results"][0]["intent"]
    assert intent["allowed"] is False
    assert "live broker requires risk limits" in intent["rejected_reason"]
    assert body["positions"] == []


def test_live_tick_allowed_once_limits_are_set(db_path):
    class Liveish(DryRunBroker):
        name = "liveish"
        is_live = True

    asyncio.run(db.set_limits(db.GLOBAL_LIMITS, max_position_value=1e9, max_daily_loss=1e9))
    body = asyncio.run(trading.run_tick("p", items(), Liveish()))
    assert body["results"][0]["intent"]["allowed"] is True


# ─── Ledger follows fills, not requests ───────────────────────────────────────

def test_partial_buy_records_only_filled_shares(db_path):
    broker = SimulatedPaperBroker(fill_ratio=0.25)
    body = asyncio.run(trading.run_tick("p", items(cash=10_000.0), broker))

    # 10_000 / 50 = 200 requested, 25% filled
    intent = body["results"][0]["intent"]
    assert intent["qty"] == pytest.approx(200.0)
    assert intent["order"]["filled_qty"] == pytest.approx(50.0)
    assert body["positions"][0]["qty"] == pytest.approx(50.0)


def test_rejected_order_leaves_ledger_untouched(db_path):
    broker = SimulatedPaperBroker(reject_symbols={"BUY"})
    body = asyncio.run(trading.run_tick("p", items(), broker))

    assert body["results"][0]["intent"]["order"]["status"] == "rejected"
    assert body["positions"] == []


def test_order_row_is_persisted(db_path):
    asyncio.run(trading.run_tick("p", items(), SimulatedPaperBroker()))
    orders = asyncio.run(db.list_orders("p"))

    assert len(orders) == 1
    assert orders[0]["status"] == "filled"
    assert orders[0]["broker"] == "simulated_paper"
    assert orders[0]["broker_order_id"] == "sim-1"
    assert orders[0]["intent_id"] is not None


def test_partial_fill_uses_fill_price_not_signal_price(db_path):
    broker = SimulatedPaperBroker(slippage=0.02)
    asyncio.run(trading.run_tick("p", items(), broker))

    position = asyncio.run(db.get_position("p", "BUY", RSI2_KEY))
    assert position["avg_price"] == pytest.approx(51.0)  # 50 * 1.02


# ─── Open-order sync ──────────────────────────────────────────────────────────

def test_sync_completes_a_partially_filled_order(db_path):
    """A partial that later completes must top the ledger up, not double it."""
    broker = SimulatedPaperBroker(fill_ratio=0.5)
    asyncio.run(trading.run_tick("p", items(), broker))

    position = asyncio.run(db.get_position("p", "BUY", RSI2_KEY))
    assert position["qty"] == pytest.approx(100.0)  # half of 200

    # Venue completes the order between ticks
    order = broker._orders["sim-1"]
    order.status = "filled"
    order.filled_qty = 200.0

    synced = asyncio.run(trading.sync_open_orders("p", broker))
    assert len(synced) == 1
    assert synced[0]["newly_filled"] == pytest.approx(100.0)

    position = asyncio.run(db.get_position("p", "BUY", RSI2_KEY))
    assert position["qty"] == pytest.approx(200.0)


def test_sync_ignores_terminal_orders(db_path):
    asyncio.run(trading.run_tick("p", items(), SimulatedPaperBroker()))
    assert asyncio.run(trading.sync_open_orders("p", SimulatedPaperBroker())) == []


def test_open_orders_endpoint_lists_only_open(client):
    client.post("/trade/tick", json={
        "plan_id": "p", "items": [{"dataset": "BUY.csv", "strategy": RSI2, "cash_allocation": 100}],
    })
    all_orders = client.get("/trade/orders?plan_id=p").json()["orders"]
    open_orders = client.get("/trade/orders?plan_id=p&open_only=true").json()["orders"]

    assert len(all_orders) == 1
    assert open_orders == []  # DryRunBroker fills instantly


# ─── Reconciliation ───────────────────────────────────────────────────────────

def test_reconcile_reports_in_sync(db_path):
    broker = SimulatedPaperBroker()
    asyncio.run(trading.run_tick("p", items(), broker))

    rec = asyncio.run(trading.reconcile("p", broker))
    assert rec["in_sync"] is True
    assert rec["drift"] == []


def test_reconcile_detects_quantity_drift(db_path):
    broker = SimulatedPaperBroker()
    asyncio.run(trading.run_tick("p", items(), broker))

    # Venue silently loses half the position
    broker._positions["BUY"]["qty"] /= 2

    rec = asyncio.run(trading.reconcile("p", broker))
    assert rec["in_sync"] is False
    assert rec["drift"][0]["symbol"] == "BUY"
    assert rec["drift"][0]["delta"] == pytest.approx(-100.0)


def test_reconcile_detects_position_only_at_broker(db_path):
    """A holding we don't know about is drift, not an omission."""
    broker = SimulatedPaperBroker()
    apply_fill(broker._positions, "GHOST", "buy", 5.0, 10.0)

    rec = asyncio.run(trading.reconcile("p", broker))
    assert rec["in_sync"] is False
    assert rec["drift"][0]["symbol"] == "GHOST"
    assert rec["drift"][0]["ledger_qty"] == 0.0


def test_reconcile_detects_position_only_in_ledger(db_path):
    asyncio.run(db.upsert_position("p", "ORPHAN", RSI2_KEY, 7.0, 10.0))

    rec = asyncio.run(trading.reconcile("p", SimulatedPaperBroker()))
    assert rec["in_sync"] is False
    assert rec["drift"][0]["symbol"] == "ORPHAN"
    assert rec["drift"][0]["broker_qty"] == 0.0


def test_reconcile_tolerates_float_noise(db_path):
    broker = SimulatedPaperBroker()
    asyncio.run(trading.run_tick("p", items(), broker))
    broker._positions["BUY"]["qty"] += 1e-9

    assert asyncio.run(trading.reconcile("p", broker))["in_sync"] is True


def test_ledger_by_symbol_aggregates_across_strategies(db_path):
    asyncio.run(db.upsert_position("p", "X", "strat-a", 10.0, 100.0))
    asyncio.run(db.upsert_position("p", "X", "strat-b", 5.0, 100.0))

    assert asyncio.run(db.ledger_by_symbol("p")) == {"X": 15.0}


def test_tick_response_includes_reconciliation(client):
    body = client.post("/trade/tick", json={
        "plan_id": "p", "items": [{"dataset": "BUY.csv", "strategy": RSI2, "cash_allocation": 100}],
    }).json()

    assert body["reconciliation"]["in_sync"] is True
    assert body["reconciliation"]["broker"] == "dry_run"
