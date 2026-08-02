import asyncio
import csv
import json
import os
from datetime import date, timedelta
from pathlib import Path

import pytest
from fastapi.testclient import TestClient


# ─── Candle helpers ───────────────────────────────────────────────────────────

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


# Big drop → RSI(2) emits "buy"; big gain → RSI(2) emits "sell"; flat → MA never crosses
BUY_CANDLES = make_candles([100.0] * 30 + [50.0])
SELL_CANDLES = make_candles([100.0] * 30 + [200.0])
HOLD_CANDLES = make_candles([100.0] * 30)


# ─── Pytest fixtures ──────────────────────────────────────────────────────────

@pytest.fixture()
def data_dir(tmp_path):
    d = tmp_path / "datasets"
    d.mkdir()
    write_csv(d / "BUY.csv", BUY_CANDLES)
    write_csv(d / "SELL.csv", SELL_CANDLES)
    write_csv(d / "HOLD.csv", HOLD_CANDLES)
    return d


@pytest.fixture()
def client(monkeypatch, tmp_path, data_dir):
    db_file = str(tmp_path / "test.db")
    monkeypatch.setenv("RUSTY_FINANCE_DB", db_file)
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(data_dir))
    from api.main import app
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


RSI2 = {"type": "rsi", "period": 2}
MA_HOLD = {"type": "ma_ema", "short_window": 3, "long_window": 5}


# ─── decide() unit tests ──────────────────────────────────────────────────────

def test_decide_buy_when_flat():
    from api.trading import decide
    intent = decide("buy", None, 100.0, 10_000.0, "TEST", "s")
    assert intent is not None
    assert intent.side == "buy"
    assert abs(intent.qty - 100.0) < 1e-6
    assert intent.price == 100.0


def test_decide_buy_when_long_is_noop():
    from api.trading import decide
    assert decide("buy", {"qty": 10.0}, 100.0, 10_000.0, "TEST", "s") is None


def test_decide_sell_when_long():
    from api.trading import decide
    intent = decide("sell", {"qty": 10.0}, 100.0, 10_000.0, "TEST", "s")
    assert intent is not None
    assert intent.side == "sell"
    assert intent.qty == 10.0


def test_decide_sell_when_flat_is_noop():
    from api.trading import decide
    assert decide("sell", None, 100.0, 10_000.0, "TEST", "s") is None


def test_decide_hold_is_noop():
    from api.trading import decide
    assert decide("hold", {"qty": 10.0}, 100.0, 10_000.0, "TEST", "s") is None


# ─── DryRunBroker ─────────────────────────────────────────────────────────────

def test_dryrun_broker_fills_completely():
    from api.broker import DryRunBroker, OrderIntent
    broker = DryRunBroker()
    intent = OrderIntent(symbol="X", side="buy", qty=1.0, price=100.0, reason="test", strategy="s")
    order = asyncio.run(broker.submit(intent))

    assert order.status == "filled"
    assert order.filled_qty == 1.0
    assert order.avg_fill_price == 100.0
    assert order.is_terminal


def test_dryrun_broker_is_not_live():
    """is_live gates the fail-closed risk check, so it must be honest."""
    from api.broker import DryRunBroker
    assert DryRunBroker().is_live is False


def test_dryrun_broker_never_raises():
    from api.broker import DryRunBroker, OrderIntent
    broker = DryRunBroker()
    for side in ("buy", "sell"):
        intent = OrderIntent(symbol="X", side=side, qty=1.0, price=100.0, reason="r", strategy="s")
        asyncio.run(broker.submit(intent))  # must not raise


# ─── latest_signal (Rust primitive) ──────────────────────────────────────────

def test_latest_signal_buy():
    bt = pytest.importorskip("backtesting_py")
    candles = json.dumps(BUY_CANDLES)
    result = json.loads(bt.latest_signal('{"type":"rsi","period":2}', candles))
    assert result["signal"] == "buy"
    assert result["close"] == 50.0
    assert result["bars"] == len(BUY_CANDLES)


def test_latest_signal_sell():
    bt = pytest.importorskip("backtesting_py")
    candles = json.dumps(SELL_CANDLES)
    result = json.loads(bt.latest_signal('{"type":"rsi","period":2}', candles))
    assert result["signal"] == "sell"
    assert result["close"] == 200.0


def test_latest_signal_empty_candles_raises():
    bt = pytest.importorskip("backtesting_py")
    with pytest.raises(Exception):
        bt.latest_signal('{"type":"rsi","period":2}', "[]")


# ─── /trade/tick integration tests ───────────────────────────────────────────

def test_tick_buy_signal_creates_intent(client):
    pytest.importorskip("backtesting_py")
    resp = client.post("/trade/tick", json={
        "plan_id": "p1",
        "items": [{"dataset": "BUY.csv", "strategy": RSI2, "cash_allocation": 10000}],
    })
    assert resp.status_code == 200
    body = resp.json()
    assert body["plan_id"] == "p1"
    r = body["results"][0]
    assert r["signal"] == "buy"
    assert r["intent"] is not None
    assert r["intent"]["side"] == "buy"
    assert r["intent"]["status"] == "filled"

    # position should be recorded
    positions = body["positions"]
    assert len(positions) == 1
    assert positions[0]["symbol"] == "BUY"
    assert positions[0]["qty"] > 0


def test_tick_idempotent_no_duplicate_buy(client):
    pytest.importorskip("backtesting_py")
    payload = {
        "plan_id": "p2",
        "items": [{"dataset": "BUY.csv", "strategy": RSI2, "cash_allocation": 10000}],
    }
    # First tick — creates a buy intent
    r1 = client.post("/trade/tick", json=payload)
    assert r1.status_code == 200
    assert r1.json()["results"][0]["intent"] is not None

    # Second tick — already long → no new intent
    r2 = client.post("/trade/tick", json=payload)
    assert r2.status_code == 200
    assert r2.json()["results"][0]["intent"] is None

    # Only one intent total in the ledger
    intents = client.get("/trade/intents", params={"plan_id": "p2"}).json()["intents"]
    assert len(intents) == 1


def test_tick_sell_exits_position(client, monkeypatch, tmp_path):
    pytest.importorskip("backtesting_py")
    import api.db as db
    from api.datasets import _data_dir

    # Compute the strategy key the endpoint will use for RSI(2)
    rsi2_key = json.dumps({"period": 2, "type": "rsi"}, sort_keys=True)

    # Seed a long position for SELL symbol
    asyncio.run(db.upsert_position("p3", "SELL", rsi2_key, qty=50.0, avg_price=200.0))

    resp = client.post("/trade/tick", json={
        "plan_id": "p3",
        "items": [{"dataset": "SELL.csv", "strategy": RSI2, "cash_allocation": 10000}],
    })
    assert resp.status_code == 200
    r = resp.json()["results"][0]
    assert r["signal"] == "sell"
    assert r["intent"] is not None
    assert r["intent"]["side"] == "sell"
    assert r["intent"]["qty"] == 50.0

    # Position should now be flat
    positions = resp.json()["positions"]
    sell_pos = next(p for p in positions if p["symbol"] == "SELL")
    assert sell_pos["qty"] == 0.0


def test_tick_hold_signal_no_intent(client):
    pytest.importorskip("backtesting_py")
    resp = client.post("/trade/tick", json={
        "plan_id": "p4",
        "items": [{"dataset": "HOLD.csv", "strategy": MA_HOLD, "cash_allocation": 10000}],
    })
    assert resp.status_code == 200
    r = resp.json()["results"][0]
    assert r["signal"] == "hold"
    assert r["intent"] is None


def test_tick_unknown_dataset_404(client):
    resp = client.post("/trade/tick", json={
        "plan_id": "p5",
        "items": [{"dataset": "NOPE.csv", "strategy": RSI2, "cash_allocation": 10000}],
    })
    assert resp.status_code == 404


# ─── GET /trade/intents and /trade/positions ─────────────────────────────────

def test_intents_endpoint_empty(client):
    resp = client.get("/trade/intents")
    assert resp.status_code == 200
    assert resp.json()["intents"] == []


def test_positions_endpoint_empty(client):
    resp = client.get("/trade/positions")
    assert resp.status_code == 200
    assert resp.json()["positions"] == []


def test_intents_after_tick(client):
    pytest.importorskip("backtesting_py")
    client.post("/trade/tick", json={
        "plan_id": "px",
        "items": [{"dataset": "BUY.csv", "strategy": RSI2, "cash_allocation": 10000}],
    })
    intents = client.get("/trade/intents").json()["intents"]
    assert len(intents) == 1
    assert intents[0]["side"] == "buy"
    assert intents[0]["status"] == "filled"
