import csv
import json
import os

import pytest
from fastapi.testclient import TestClient

import api.main
from api.main import app

client = TestClient(app, raise_server_exceptions=False)

FIXTURE_PATH = os.path.join(os.path.dirname(__file__), "../../data/fixtures/synthetic_30.csv")


def load_fixture_candles(path: str = FIXTURE_PATH) -> list[dict]:
    with open(path) as f:
        reader = csv.DictReader(f)
        return [
            {
                "date": row["Date"],
                "open": float(row["Open"]),
                "high": float(row["High"]),
                "low": float(row["Low"]),
                "close": float(row["Close"]),
                "volume": int(row["Volume"]),
            }
            for row in reader
        ]


CANDLES = load_fixture_candles()

MA_PAYLOAD = {"candles": CANDLES, "short_window": 3, "long_window": 5}
RSI_PAYLOAD = {"candles": CANDLES, "period": 7}


# ---------------------------------------------------------------------------
# Health endpoint
# ---------------------------------------------------------------------------

def test_health_returns_200():
    resp = client.get("/health")
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"


def test_health_engine_field_present():
    body = client.get("/health").json()
    assert body["engine"] in ("available", "unavailable")


# ---------------------------------------------------------------------------
# 503 when engine unavailable
# ---------------------------------------------------------------------------

def test_ma_returns_503_when_engine_unavailable(monkeypatch):
    monkeypatch.setattr(api.main, "_ENGINE_AVAILABLE", False)
    resp = client.post("/backtest/ma", json=MA_PAYLOAD)
    assert resp.status_code == 503
    assert "not installed" in resp.json()["detail"]


def test_rsi_returns_503_when_engine_unavailable(monkeypatch):
    monkeypatch.setattr(api.main, "_ENGINE_AVAILABLE", False)
    resp = client.post("/backtest/rsi", json=RSI_PAYLOAD)
    assert resp.status_code == 503


# ---------------------------------------------------------------------------
# Validation errors (no engine required)
# ---------------------------------------------------------------------------

def test_ma_missing_candles_returns_422():
    resp = client.post("/backtest/ma", json={"short_window": 5, "long_window": 20})
    assert resp.status_code == 422


def test_rsi_missing_candles_returns_422():
    resp = client.post("/backtest/rsi", json={"period": 14})
    assert resp.status_code == 422


def test_ma_negative_commission_returns_422():
    payload = {**MA_PAYLOAD, "commission": -1.0}
    resp = client.post("/backtest/ma", json=payload)
    assert resp.status_code == 422


# ---------------------------------------------------------------------------
# Engine-dependent tests (auto-skipped if backtesting_py not installed)
# ---------------------------------------------------------------------------

def test_ma_returns_full_result_shape():
    pytest.importorskip("backtesting_py")
    resp = client.post("/backtest/ma", json=MA_PAYLOAD)
    assert resp.status_code == 200
    body = resp.json()
    assert "equity_curve" in body
    assert "trades" in body
    assert "metrics" in body
    assert "benchmark" in body
    m = body["metrics"]
    assert isinstance(m["total_return"], float)
    assert isinstance(m["cagr"], float)
    assert isinstance(m["max_drawdown"], float)
    assert isinstance(m["sharpe_ratio"], float)
    assert isinstance(m["sortino_ratio"], float)
    assert isinstance(m["trade_count"], int)


def test_rsi_returns_full_result_shape():
    pytest.importorskip("backtesting_py")
    resp = client.post("/backtest/rsi", json=RSI_PAYLOAD)
    assert resp.status_code == 200
    body = resp.json()
    assert all(k in body for k in ("equity_curve", "trades", "metrics", "benchmark"))


def test_ma_equity_curve_length_matches_candle_count():
    pytest.importorskip("backtesting_py")
    resp = client.post("/backtest/ma", json=MA_PAYLOAD)
    body = resp.json()
    assert len(body["equity_curve"]) == len(CANDLES)


def test_benchmark_present_and_finite():
    pytest.importorskip("backtesting_py")
    resp = client.post("/backtest/ma", json=MA_PAYLOAD)
    bm = resp.json()["benchmark"]
    assert isinstance(bm["total_return"], float)
    assert isinstance(bm["cagr"], float)


def test_commission_reduces_pnl():
    pytest.importorskip("backtesting_py")
    base = client.post("/backtest/ma", json=MA_PAYLOAD).json()
    with_cost = client.post("/backtest/ma", json={**MA_PAYLOAD, "commission": 10.0}).json()
    # With commission, final NAV should be lower or equal
    base_nav = base["equity_curve"][-1]["nav"]
    cost_nav = with_cost["equity_curve"][-1]["nav"]
    assert cost_nav <= base_nav


def test_ma_nonexistent_csv_path_not_accepted():
    # The new API doesn't accept csv_path at all — passing it as candles should 422
    resp = client.post("/backtest/ma", json={"csv_path": "/tmp/nope.csv"})
    assert resp.status_code == 422
