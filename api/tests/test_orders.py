"""Order lifecycle, fail-closed risk, and position reconciliation.

The ledger must follow what a venue actually *filled*, not what we asked for —
these tests drive partial fills and rejections through the real tick path via
SimulatedPaperBroker.
"""

import asyncio
import csv
import json
from datetime import date, datetime, timedelta, timezone
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


def _today() -> str:
    """The day key daily_stats buckets on — UTC, matching run_tick."""
    return datetime.now(timezone.utc).date().isoformat()


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


def test_a_topped_up_exit_reports_its_realized_pnl(db_path, data_dir):
    """PnL realized by a top-up fill must reach daily_stats, or max_daily_loss
    is blind to the second half of every partial exit.

    Realized PnL used to be computed once at submit time from the filled
    quantity and written onto the intent row. A later top-up moved the ledger
    correctly but recorded no PnL anywhere, so a partial exit that completed on
    a subsequent tick contributed only its first half to the loss cap.
    """
    # Enter fully at 50, so the position is 200 shares at 50.
    broker = SimulatedPaperBroker()
    asyncio.run(trading.run_tick("p", items(), broker))
    assert asyncio.run(db.get_position("p", "BUY", RSI2_KEY))["qty"] == pytest.approx(200.0)

    entry_pnl = asyncio.run(db.daily_stats("p", _today()))["realized_pnl"]

    # Exit at 200, but the venue only fills half.
    write_csv(data_dir / "BUY.csv", make_candles([100.0] * 30 + [200.0]))
    half = SimulatedPaperBroker(fill_ratio=0.5)
    asyncio.run(trading.run_tick("p", items(), half))

    after_partial = asyncio.run(db.daily_stats("p", _today()))["realized_pnl"]
    # 100 of 200 shares sold at 200 against a 50 cost basis.
    assert after_partial - entry_pnl == pytest.approx(15_000.0)

    # The venue completes the remaining 100 shares between ticks.
    open_rows = asyncio.run(db.list_open_orders("p"))
    assert len(open_rows) == 1
    order = half._orders[open_rows[0]["broker_order_id"]]
    order.status = "filled"
    order.filled_qty = 200.0

    synced = asyncio.run(trading.sync_open_orders("p", half))
    assert synced[0]["newly_filled"] == pytest.approx(100.0)

    after_topup = asyncio.run(db.daily_stats("p", _today()))["realized_pnl"]
    assert after_topup - entry_pnl == pytest.approx(30_000.0), (
        "the topped-up half realized 15,000 more and daily_stats must see it"
    )


def test_soak_realized_pnl_includes_topped_up_fills(db_path, data_dir):
    """/trade/soak reports realized PnL from the same source as the loss cap, so
    a partial exit completed on a later tick is counted in full."""
    asyncio.run(trading.run_tick("p", items(), SimulatedPaperBroker()))

    write_csv(data_dir / "BUY.csv", make_candles([100.0] * 30 + [200.0]))
    half = SimulatedPaperBroker(fill_ratio=0.5)
    asyncio.run(trading.run_tick("p", items(), half))

    open_rows = asyncio.run(db.list_open_orders("p"))
    order = half._orders[open_rows[0]["broker_order_id"]]
    order.status = "filled"
    order.filled_qty = 200.0
    asyncio.run(trading.sync_open_orders("p", half))

    assert asyncio.run(db.total_realized_pnl("p")) == pytest.approx(30_000.0)


def test_venue_rejections_consume_the_daily_order_budget(db_path):
    """A venue rejection reached the broker, so it must count against
    max_daily_orders.

    daily_stats used to filter `status NOT LIKE 'rejected%'` to keep *pre-trade*
    risk rejections from burning the budget — correct, since those never left the
    process. But a venue rejection lands in the same status, so a symbol the
    venue rejects on every tick incremented nothing and max_daily_orders could
    never fire on it. Repeated venue rejections are the most likely runaway loop
    in practice, which is exactly what that cap exists to bound.
    """
    broker = SimulatedPaperBroker(reject_symbols={"BUY"})
    asyncio.run(trading.run_tick("p", items(), broker))

    orders = asyncio.run(db.list_orders(plan_id="p"))
    assert len(orders) == 1 and orders[0]["status"] == "rejected"

    stats = asyncio.run(db.daily_stats("p", _today()))
    assert stats["orders"] == 1, (
        "a venue rejection reached the broker and must consume order budget"
    )


def test_repeated_venue_rejections_trip_the_order_cap(db_path):
    """The runaway this cap exists to bound, end to end.

    A symbol the venue rejects on every tick produces an order every time. With
    the budget counted correctly, the cap eventually refuses to submit; while
    venue rejections were invisible the loop could retry forever.
    """
    asyncio.run(db.set_limits(db.GLOBAL_LIMITS, max_daily_orders=2))
    broker = SimulatedPaperBroker(reject_symbols={"BUY"})

    for _ in range(2):
        asyncio.run(trading.run_tick("p", items(), broker))
    assert asyncio.run(db.daily_stats("p", _today()))["orders"] == 2

    third = asyncio.run(trading.run_tick("p", items(), broker))
    intent = third["results"][0]["intent"]
    assert intent["allowed"] is False
    assert "max_daily_orders" in intent["status"]
    # The refused attempt never reached the venue, so still exactly two orders.
    assert len(asyncio.run(db.list_orders(plan_id="p"))) == 2


def test_a_topped_up_loss_trips_the_daily_loss_cap(db_path, data_dir):
    """The loss-cap counterpart: a loss realized by a top-up must be able to
    block the next entry, not just be recorded."""
    # Enter at 50 (200 shares), then exit at 10 with only half filling.
    asyncio.run(trading.run_tick("p", items(), SimulatedPaperBroker()))
    write_csv(data_dir / "BUY.csv", make_candles([5.0] * 30 + [10.0]))
    half = SimulatedPaperBroker(fill_ratio=0.5)
    asyncio.run(trading.run_tick("p", items(), half))

    # Cap the loss above what the first half realized (-4,000) but below the
    # full exit (-8,000), so only the topped-up half can trip it.
    asyncio.run(db.set_limits(db.GLOBAL_LIMITS, max_daily_loss=6_000.0))
    assert asyncio.run(db.daily_stats("p", _today()))["realized_pnl"] == pytest.approx(-4_000.0)

    open_rows = asyncio.run(db.list_open_orders("p"))
    order = half._orders[open_rows[0]["broker_order_id"]]
    order.status = "filled"
    order.filled_qty = 200.0
    asyncio.run(trading.sync_open_orders("p", half))

    assert asyncio.run(db.daily_stats("p", _today()))["realized_pnl"] == pytest.approx(-8_000.0)

    # Now a fresh entry must be refused by the loss cap.
    write_csv(data_dir / "BUY.csv", BUY_CANDLES)
    body = asyncio.run(trading.run_tick("p", items(), SimulatedPaperBroker()))
    intent = body["results"][0]["intent"]
    assert intent["allowed"] is False
    assert "max_daily_loss" in intent["status"]


def test_pretrade_rejections_do_not_consume_the_budget(db_path):
    """The other side of the asymmetry: a risk rejection sent nothing, so it must
    not burn the budget — otherwise one misconfigured limit locks the plan out."""
    asyncio.run(db.set_limits(db.GLOBAL_LIMITS, max_position_value=1.0))
    asyncio.run(trading.run_tick("p", items(), SimulatedPaperBroker()))

    assert asyncio.run(db.list_orders(plan_id="p")) == []
    assert asyncio.run(db.daily_stats("p", _today()))["orders"] == 0


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
