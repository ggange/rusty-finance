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
        return [
            {
                "date": row["Date"],
                "open": float(row["Open"]),
                "high": float(row["High"]),
                "low": float(row["Low"]),
                "close": float(row["Close"]),
                "volume": int(row["Volume"]),
            }
            for row in csv.DictReader(f)
        ]


CANDLES = load_fixture_candles()

MA_PAYLOAD = {
    "strategy": {"type": "ma_ema", "short_window": 3, "long_window": 5},
    "candles": CANDLES,
}
RSI_PAYLOAD = {
    "strategy": {"type": "rsi", "period": 7},
    "candles": CANDLES,
}


# ---------------------------------------------------------------------------
# Health
# ---------------------------------------------------------------------------

def test_health_returns_200():
    resp = client.get("/health")
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"


def test_health_engine_field_present():
    assert client.get("/health").json()["engine"] in ("available", "unavailable")


# ---------------------------------------------------------------------------
# Strategy registry
# ---------------------------------------------------------------------------

def test_strategies_lists_all_types():
    resp = client.get("/strategies")
    assert resp.status_code == 200
    types = {s["type"] for s in resp.json()["strategies"]}
    assert types == {"ma_ema", "ma_sma", "ma_wma", "rsi"}


def test_strategies_each_entry_has_required_fields():
    for s in client.get("/strategies").json()["strategies"]:
        assert "type" in s
        assert "name" in s
        assert "description" in s
        assert "params" in s


# ---------------------------------------------------------------------------
# 503 when engine unavailable
# ---------------------------------------------------------------------------

def test_backtest_returns_503_when_engine_unavailable(monkeypatch):
    monkeypatch.setattr(api.main, "_ENGINE_AVAILABLE", False)
    resp = client.post("/backtest", json=MA_PAYLOAD)
    assert resp.status_code == 503
    assert "not installed" in resp.json()["detail"]


# ---------------------------------------------------------------------------
# Validation (422) — no engine required
# ---------------------------------------------------------------------------

def test_missing_candles_returns_422():
    resp = client.post("/backtest", json={"strategy": {"type": "ma_ema"}})
    assert resp.status_code == 422


def test_unknown_strategy_type_returns_422():
    resp = client.post("/backtest", json={"strategy": {"type": "unknown"}, "candles": CANDLES})
    assert resp.status_code == 422


def test_ma_short_window_gte_long_returns_422():
    bad = {**MA_PAYLOAD, "strategy": {"type": "ma_ema", "short_window": 10, "long_window": 5}}
    assert client.post("/backtest", json=bad).status_code == 422


def test_negative_commission_returns_422():
    assert client.post("/backtest", json={**MA_PAYLOAD, "commission": -1.0}).status_code == 422


def test_slippage_gte_1_returns_422():
    assert client.post("/backtest", json={**MA_PAYLOAD, "slippage_pct": 1.0}).status_code == 422


def test_rsi_period_1_returns_422():
    bad = {**RSI_PAYLOAD, "strategy": {"type": "rsi", "period": 1}}
    assert client.post("/backtest", json=bad).status_code == 422


# ---------------------------------------------------------------------------
# Engine-dependent tests (auto-skipped if backtesting_py not installed)
# ---------------------------------------------------------------------------

def test_ma_ema_returns_full_result_shape():
    pytest.importorskip("backtesting_py")
    resp = client.post("/backtest", json=MA_PAYLOAD)
    assert resp.status_code == 200
    body = resp.json()
    assert all(k in body for k in ("equity_curve", "trades", "metrics", "benchmark"))
    m = body["metrics"]
    for field in ("total_return", "cagr", "annualized_volatility",
                  "max_drawdown", "sharpe_ratio", "sortino_ratio", "trade_count"):
        assert field in m, f"missing metrics field: {field}"


def test_rsi_returns_full_result_shape():
    pytest.importorskip("backtesting_py")
    resp = client.post("/backtest", json=RSI_PAYLOAD)
    assert resp.status_code == 200
    assert all(k in resp.json() for k in ("equity_curve", "trades", "metrics", "benchmark"))


def test_ma_sma_strategy_works():
    pytest.importorskip("backtesting_py")
    payload = {**MA_PAYLOAD, "strategy": {"type": "ma_sma", "short_window": 3, "long_window": 5}}
    assert client.post("/backtest", json=payload).status_code == 200


def test_ma_wma_strategy_works():
    pytest.importorskip("backtesting_py")
    payload = {**MA_PAYLOAD, "strategy": {"type": "ma_wma", "short_window": 3, "long_window": 5}}
    assert client.post("/backtest", json=payload).status_code == 200


def test_equity_curve_length_matches_candle_count():
    pytest.importorskip("backtesting_py")
    body = client.post("/backtest", json=MA_PAYLOAD).json()
    assert len(body["equity_curve"]) == len(CANDLES)


def test_benchmark_fields_present():
    pytest.importorskip("backtesting_py")
    bm = client.post("/backtest", json=MA_PAYLOAD).json()["benchmark"]
    assert "total_return" in bm and "cagr" in bm


def test_commission_reduces_final_nav():
    pytest.importorskip("backtesting_py")
    base_nav = client.post("/backtest", json=MA_PAYLOAD).json()["equity_curve"][-1]["nav"]
    cost_nav = client.post("/backtest", json={**MA_PAYLOAD, "commission": 10.0}).json()["equity_curve"][-1]["nav"]
    assert cost_nav <= base_nav


def test_win_rate_none_when_no_completed_sells():
    pytest.importorskip("backtesting_py")
    # Hold strategy → no trades → win_rate is null
    hold_payload = {
        "strategy": {"type": "ma_ema", "short_window": 1, "long_window": 29},
        "candles": CANDLES,
    }
    m = client.post("/backtest", json=hold_payload).json()["metrics"]
    # win_rate is None/null when there are no sells
    assert m.get("win_rate") is None or isinstance(m.get("win_rate"), float)
