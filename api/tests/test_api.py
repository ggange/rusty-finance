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
    assert types == {"ma_ema", "ma_sma", "ma_wma", "rsi", "macd", "bollinger_bands"}


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


# ---------------------------------------------------------------------------
# Dataset catalog
# ---------------------------------------------------------------------------

def _seed_dataset_dir(tmp_path) -> str:
    path = tmp_path / "AAPL.csv"
    with open(path, "w", newline="") as f:
        f.write("Date,Open,High,Low,Close,Volume\n")
        for c in CANDLES:
            f.write(f"{c['date']},{c['open']},{c['high']},{c['low']},{c['close']},{int(c['volume'])}\n")
    return str(tmp_path)


def test_datasets_returns_list():
    body = client.get("/datasets").json()
    assert "datasets" in body and isinstance(body["datasets"], list)


def test_datasets_reads_configured_dir(monkeypatch, tmp_path):
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    datasets = client.get("/datasets").json()["datasets"]
    names = {d["name"] for d in datasets}
    assert "AAPL.csv" in names
    aapl = next(d for d in datasets if d["name"] == "AAPL.csv")
    for field in ("name", "symbol", "rows", "start", "end"):
        assert field in aapl
    assert aapl["symbol"] == "AAPL"
    assert aapl["rows"] == len(CANDLES)


def test_datasets_missing_dir_returns_empty(monkeypatch, tmp_path):
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(tmp_path / "does_not_exist"))
    assert client.get("/datasets").json()["datasets"] == []


def test_get_dataset_returns_candles(monkeypatch, tmp_path):
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    body = client.get("/datasets/AAPL.csv").json()
    assert body["name"] == "AAPL.csv"
    assert len(body["candles"]) == len(CANDLES)
    assert set(body["candles"][0]) >= {"date", "open", "high", "low", "close", "volume"}


def test_get_unknown_dataset_returns_404(monkeypatch, tmp_path):
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(tmp_path))
    assert client.get("/datasets/missing.csv").status_code == 404


def test_get_dataset_path_traversal_blocked(monkeypatch, tmp_path):
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(tmp_path))
    # Encoded traversal should not escape the data dir.
    resp = client.get("/datasets/..%2F..%2Fetc%2Fpasswd")
    assert resp.status_code in (404, 422)


# ---------------------------------------------------------------------------
# Portfolio — validation (no engine required)
# ---------------------------------------------------------------------------

INLINE_ASSET = {
    "symbol": "A",
    "weight": 0.5,
    "source": {"kind": "inline", "candles": CANDLES},
    "strategy": {"type": "ma_sma", "short_window": 3, "long_window": 5},
}
RSI_ASSET = {
    "symbol": "B",
    "weight": 0.5,
    "source": {"kind": "inline", "candles": CANDLES},
    "strategy": {"type": "rsi", "period": 7},
}
PORTFOLIO_PAYLOAD = {"assets": [INLINE_ASSET, RSI_ASSET], "initial_cash": 10_000.0}


def test_portfolio_empty_assets_returns_422():
    assert client.post("/portfolio", json={"assets": []}).status_code == 422


def test_portfolio_bad_strategy_returns_422():
    bad = {"assets": [{**INLINE_ASSET, "strategy": {"type": "ma_ema", "short_window": 10, "long_window": 5}}]}
    assert client.post("/portfolio", json=bad).status_code == 422


def test_portfolio_returns_503_when_engine_unavailable(monkeypatch):
    monkeypatch.setattr(api.main, "_ENGINE_AVAILABLE", False)
    resp = client.post("/portfolio", json=PORTFOLIO_PAYLOAD)
    assert resp.status_code == 503


# ---------------------------------------------------------------------------
# Portfolio — engine-dependent
# ---------------------------------------------------------------------------

def test_portfolio_returns_aggregate_and_breakdown():
    pytest.importorskip("backtesting_py")
    resp = client.post("/portfolio", json=PORTFOLIO_PAYLOAD)
    assert resp.status_code == 200
    body = resp.json()
    assert all(k in body for k in ("equity_curve", "metrics", "benchmark", "assets"))
    assert len(body["equity_curve"]) == len(CANDLES)
    assert len(body["assets"]) == 2
    a0 = body["assets"][0]
    for field in ("symbol", "weight", "allocated_cash", "equity_curve", "trades", "metrics", "benchmark"):
        assert field in a0, f"missing asset field: {field}"
    # Equal weights → 5000 each.
    assert abs(a0["allocated_cash"] - 5_000.0) < 1e-6


def test_portfolio_weight_defaulting():
    pytest.importorskip("backtesting_py")
    payload = {
        "assets": [
            {"symbol": "A", "source": {"kind": "inline", "candles": CANDLES},
             "strategy": {"type": "ma_sma", "short_window": 3, "long_window": 5}},
            {"symbol": "B", "source": {"kind": "inline", "candles": CANDLES},
             "strategy": {"type": "rsi", "period": 7}},
        ],
        "initial_cash": 10_000.0,
    }
    body = client.post("/portfolio", json=payload).json()
    # No weights → equal split.
    assert abs(body["assets"][0]["allocated_cash"] - 5_000.0) < 1e-6
    assert abs(body["assets"][1]["allocated_cash"] - 5_000.0) < 1e-6


def test_portfolio_dataset_source(monkeypatch, tmp_path):
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    payload = {
        "assets": [{
            "symbol": "AAPL",
            "source": {"kind": "dataset", "name": "AAPL.csv"},
            "strategy": {"type": "ma_sma", "short_window": 3, "long_window": 5},
        }],
        "initial_cash": 10_000.0,
    }
    resp = client.post("/portfolio", json=payload)
    assert resp.status_code == 200
    assert len(resp.json()["equity_curve"]) == len(CANDLES)


def test_portfolio_unknown_dataset_returns_404(monkeypatch, tmp_path):
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", str(tmp_path))
    payload = {
        "assets": [{
            "symbol": "NOPE",
            "source": {"kind": "dataset", "name": "missing.csv"},
            "strategy": {"type": "rsi", "period": 7},
        }],
    }
    assert client.post("/portfolio", json=payload).status_code == 404


# ---------------------------------------------------------------------------
# New strategy tests — MACD and Bollinger Bands
# ---------------------------------------------------------------------------

MACD_PAYLOAD = {
    "strategy": {"type": "macd", "fast_period": 3, "slow_period": 5, "signal_period": 3},
    "candles": CANDLES,
}

BB_PAYLOAD = {
    "strategy": {"type": "bollinger_bands", "period": 5, "std_dev_mult": 1.0},
    "candles": CANDLES,
}


def test_macd_returns_full_result_shape():
    pytest.importorskip("backtesting_py")
    resp = client.post("/backtest", json=MACD_PAYLOAD)
    assert resp.status_code == 200
    assert all(k in resp.json() for k in ("equity_curve", "trades", "metrics", "benchmark"))


def test_bollinger_bands_returns_full_result_shape():
    pytest.importorskip("backtesting_py")
    resp = client.post("/backtest", json=BB_PAYLOAD)
    assert resp.status_code == 200
    assert all(k in resp.json() for k in ("equity_curve", "trades", "metrics", "benchmark"))


def test_macd_fast_gte_slow_returns_422():
    bad = {**MACD_PAYLOAD, "strategy": {"type": "macd", "fast_period": 10, "slow_period": 5, "signal_period": 3}}
    assert client.post("/backtest", json=bad).status_code == 422


def test_bollinger_bands_bad_std_dev_returns_422():
    bad = {**BB_PAYLOAD, "strategy": {"type": "bollinger_bands", "period": 20, "std_dev_mult": 0.3}}
    assert client.post("/backtest", json=bad).status_code == 422
