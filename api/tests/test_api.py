import csv
import json
import os
from datetime import date

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


def test_portfolio_includes_risk_block():
    pytest.importorskip("backtesting_py")
    resp = client.post("/portfolio", json=PORTFOLIO_PAYLOAD)
    assert resp.status_code == 200
    body = resp.json()
    risk = body.get("risk")
    assert risk is not None, "response missing 'risk' key"
    for key in ("correlation", "covariance", "contribution_to_risk",
                "rolling_volatility", "var_95", "cvar_95", "var_99", "cvar_99"):
        assert key in risk, f"risk missing key: {key}"
    n = len(body["assets"])
    assert len(risk["correlation"]) == n, "correlation matrix wrong row count"
    assert all(len(row) == n for row in risk["correlation"]), "correlation matrix wrong col count"


def test_portfolio_external_benchmark_curve_present():
    pytest.importorskip("backtesting_py")
    payload = {**PORTFOLIO_PAYLOAD, "benchmark_symbol": "SPY.csv"}
    resp = client.post("/portfolio", json=payload)
    assert resp.status_code == 200
    body = resp.json()
    curve = body.get("external_benchmark_curve")
    assert curve is not None, "response missing 'external_benchmark_curve'"
    assert len(curve) > 0, "external_benchmark_curve is empty"
    assert "date" in curve[0] and "nav" in curve[0], "curve points must have date and nav"
    assert curve[0]["nav"] > 0, "initial NAV must be positive"


def test_portfolio_external_benchmark_unknown_dataset_returns_404():
    pytest.importorskip("backtesting_py")
    payload = {**PORTFOLIO_PAYLOAD, "benchmark_symbol": "UNKNOWN.csv"}
    resp = client.post("/portfolio", json=payload)
    assert resp.status_code == 404


# ─── /sweep ───────────────────────────────────────────────────────────────────

SWEEP_PAYLOAD = {
    "dataset": "AAPL.csv",
    "strategy_type": "ma_ema",
    "param_ranges": {
        "short_window": {"min": 5, "max": 15, "step": 5},
        "long_window": {"min": 20, "max": 40, "step": 10},
    },
}


def test_sweep_returns_results():
    pytest.importorskip("backtesting_py")
    resp = client.post("/sweep", json=SWEEP_PAYLOAD)
    assert resp.status_code == 200
    body = resp.json()
    assert "results" in body
    assert len(body["results"]) > 0
    first = body["results"][0]
    assert "params" in first and "metrics" in first
    assert "short_window" in first["params"] and "long_window" in first["params"]
    assert "sharpe_ratio" in first["metrics"]


def test_sweep_skips_invalid_combinations():
    pytest.importorskip("backtesting_py")
    # short_window overlaps long_window — all combos where short >= long are filtered
    payload = {
        "dataset": "AAPL.csv",
        "strategy_type": "ma_ema",
        "param_ranges": {
            "short_window": {"min": 10, "max": 30, "step": 10},
            "long_window": {"min": 10, "max": 30, "step": 10},
        },
    }
    resp = client.post("/sweep", json=payload)
    assert resp.status_code == 200
    results = resp.json()["results"]
    for r in results:
        assert r["params"]["short_window"] < r["params"]["long_window"]


def test_sweep_unknown_dataset_returns_404():
    pytest.importorskip("backtesting_py")
    payload = {**SWEEP_PAYLOAD, "dataset": "UNKNOWN.csv"}
    resp = client.post("/sweep", json=payload)
    assert resp.status_code == 404


# ─── Rebalancing ─────────────────────────────────────────────────────────────

def test_portfolio_monthly_rebalancing_returns_rebalance_dates():
    pytest.importorskip("backtesting_py")
    payload = {
        **PORTFOLIO_PAYLOAD,
        "assets": [
            {
                "symbol": "AAPL",
                "source": {"kind": "dataset", "name": "AAPL.csv"},
                "strategy": {"type": "ma_ema", "short_window": 5, "long_window": 20},
            },
            {
                "symbol": "SPY",
                "source": {"kind": "dataset", "name": "SPY.csv"},
                "strategy": {"type": "rsi", "period": 14},
            },
        ],
        "rebalance": {"frequency": {"kind": "monthly"}},
    }
    resp = client.post("/portfolio", json=payload)
    assert resp.status_code == 200
    body = resp.json()
    dates = body.get("rebalance_dates", [])
    assert len(dates) > 0, "monthly rebalancing should produce at least one rebalance date"
    # Each date string should be YYYY-MM-DD format
    assert all(len(d) == 10 and d[4] == "-" and d[7] == "-" for d in dates)


def test_portfolio_quarterly_rebalancing():
    pytest.importorskip("backtesting_py")
    monthly_payload = {
        **PORTFOLIO_PAYLOAD,
        "assets": [
            {"symbol": "AAPL", "source": {"kind": "dataset", "name": "AAPL.csv"}, "strategy": {"type": "ma_sma", "short_window": 5, "long_window": 20}},
            {"symbol": "MSFT", "source": {"kind": "dataset", "name": "MSFT.csv"}, "strategy": {"type": "rsi", "period": 14}},
        ],
        "rebalance": {"frequency": {"kind": "quarterly"}},
    }
    resp = client.post("/portfolio", json=monthly_payload)
    assert resp.status_code == 200
    body = resp.json()
    dates = body.get("rebalance_dates", [])
    assert len(dates) > 0, "quarterly rebalancing should produce rebalance dates"

    # Bound the count by the *actual* data span rather than a fixed number of
    # years — the catalog grows every time `make refresh` runs.
    span = client.get("/datasets/AAPL.csv").json()["candles"]
    start, end = date.fromisoformat(span[0]["date"]), date.fromisoformat(span[-1]["date"])
    quarters = ((end.year - start.year) * 12 + (end.month - start.month)) // 3 + 1
    assert len(dates) <= quarters

    # And the defining property: rebalances land about a quarter apart.
    gaps = [
        (date.fromisoformat(b) - date.fromisoformat(a)).days
        for a, b in zip(dates, dates[1:])
    ]
    assert all(80 <= g <= 100 for g in gaps), f"unexpected quarterly spacing: {gaps}"


def test_portfolio_no_rebalancing_has_no_rebalance_dates():
    pytest.importorskip("backtesting_py")
    resp = client.post("/portfolio", json=PORTFOLIO_PAYLOAD)
    assert resp.status_code == 200
    body = resp.json()
    # Without rebalancing, the field should be absent (skip_serializing_if)
    assert "rebalance_dates" not in body or body["rebalance_dates"] == []


# ─── fill_timing ─────────────────────────────────────────────────────────────

def test_fill_timing_accepted_by_backtest():
    """fill_timing field is accepted on /backtest for both valid values."""
    pytest.importorskip("backtesting_py")
    for timing in ("close", "next_open"):
        resp = client.post("/backtest", json={**MA_PAYLOAD, "fill_timing": timing})
        assert resp.status_code == 200, f"fill_timing={timing!r} should be accepted"


def test_fill_timing_invalid_value_returns_422():
    """An unknown fill_timing value should be rejected by validation."""
    bad = {**MA_PAYLOAD, "fill_timing": "unknown"}
    assert client.post("/backtest", json=bad).status_code == 422


def test_fill_timing_close_vs_next_open_produce_different_equity_curves():
    """close and next_open should produce different equity curves for a strategy that trades."""
    pytest.importorskip("backtesting_py")
    payload_close = {**MA_PAYLOAD, "fill_timing": "close"}
    payload_next  = {**MA_PAYLOAD, "fill_timing": "next_open"}
    curve_close = client.post("/backtest", json=payload_close).json()["equity_curve"]
    curve_next  = client.post("/backtest", json=payload_next).json()["equity_curve"]
    # Both have the same number of points.
    assert len(curve_close) == len(curve_next)
    # The NAVs should differ because fills happen at different prices.
    navs_close = [p["nav"] for p in curve_close]
    navs_next  = [p["nav"] for p in curve_next]
    assert navs_close != navs_next, "close and next_open must produce different equity curves"


def test_fill_timing_accepted_by_portfolio():
    pytest.importorskip("backtesting_py")
    for timing in ("close", "next_open"):
        payload = {**PORTFOLIO_PAYLOAD, "fill_timing": timing}
        assert client.post("/portfolio", json=payload).status_code == 200


def test_fill_timing_accepted_by_sweep(monkeypatch, tmp_path):
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    payload = {
        "dataset": "AAPL.csv",
        "strategy_type": "ma_ema",
        "param_ranges": {"short_window": {"min": 3, "max": 5, "step": 2}, "long_window": {"min": 6, "max": 10, "step": 4}},
        "fill_timing": "close",
    }
    assert client.post("/sweep", json=payload).status_code == 200


# ─── /walkforward ─────────────────────────────────────────────────────────────

WF_PAYLOAD = {
    "dataset": "AAPL.csv",
    "strategy_type": "ma_ema",
    "param_ranges": {
        "short_window": {"min": 3, "max": 5, "step": 2},
        "long_window":  {"min": 6, "max": 10, "step": 4},
    },
    "n_windows": 2,
    "train_frac": 0.7,
    "metric": "sharpe_ratio",
}


def test_walkforward_returns_folds(monkeypatch, tmp_path):
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    res = client.post("/walkforward", json=WF_PAYLOAD)
    assert res.status_code == 200
    body = res.json()
    assert "folds" in body
    assert len(body["folds"]) == 2


def test_walkforward_fold_has_required_fields(monkeypatch, tmp_path):
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    res = client.post("/walkforward", json=WF_PAYLOAD)
    fold = res.json()["folds"][0]
    for key in ("window_index", "train_range", "test_range", "best_params", "train_metrics", "test_metrics"):
        assert key in fold, f"missing key: {key}"
    for key in ("start", "end"):
        assert key in fold["train_range"]
        assert key in fold["test_range"]


def _seed_long_dataset_dir(tmp_path) -> str:
    """A dataset long enough for a fold's test slice to clear the bootstrap's
    minimum observation count. The shared 30-bar fixture yields 4-return test
    slices, which is deliberately below it."""
    import math

    path = tmp_path / "LONG.csv"
    with open(path, "w", newline="") as f:
        f.write("Date,Open,High,Low,Close,Volume\n")
        for i in range(240):
            close = 100.0 + 8.0 * math.sin(i / 3.0) + 0.03 * i
            day = f"2022-{1 + i // 28:02d}-{1 + i % 28:02d}"
            f.write(f"{day},{close},{close + 1},{close - 1},{close},1000\n")
    return str(tmp_path)


WF_LONG_PAYLOAD = {**WF_PAYLOAD, "dataset": "LONG.csv", "n_windows": 3}


def _interval_is_sane(iv: dict, label: str) -> None:
    assert iv["lo"] <= iv["hi"], f"{label}: lo {iv['lo']} above hi {iv['hi']}"
    assert iv["std_error"] >= 0.0, f"{label}: negative std_error"


def test_walkforward_bounds_the_out_of_sample_folds(monkeypatch, tmp_path):
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_long_dataset_dir(tmp_path))
    body = client.post("/walkforward", json=WF_LONG_PAYLOAD).json()

    for fold in body["folds"]:
        u = fold["test_metrics"]["uncertainty"]
        assert u["method"] == "stationary_bootstrap"
        assert u["confidence"] == 0.95
        assert u["observations"] > 0
        for key in ("sharpe_ratio", "sortino_ratio", "cagr"):
            _interval_is_sane(u[key], key)
        # Max drawdown gets a spread but no endpoints: block resampling destroys
        # the multi-month trends that produce deep drawdowns, so a percentile
        # interval there would be biased toward optimism.
        assert "max_drawdown_std_error" in u
        assert "max_drawdown" not in u
        # The train metric is an arg-max over the grid; bounding it would invite
        # a train-vs-test comparison that carries no information.
        assert fold["train_metrics"].get("uncertainty") is None


def test_walkforward_reports_a_pooled_out_of_sample_interval(monkeypatch, tmp_path):
    """The honest headline for a walk-forward run.

    Pooling treats the fold sequence as the single dependent return path it is,
    rather than as N independent observations, so the pooled series is longer than
    any one fold and supports a correspondingly tighter statement.
    """
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_long_dataset_dir(tmp_path))
    body = client.post("/walkforward", json=WF_LONG_PAYLOAD).json()

    pooled = body["oos_metrics"]
    assert pooled is not None
    u = pooled["uncertainty"]
    _interval_is_sane(u["sharpe_ratio"], "pooled sharpe")

    per_fold = sum(f["test_metrics"]["uncertainty"]["observations"] for f in body["folds"])
    assert u["observations"] == per_fold, "pooled path must be every fold's returns"


def test_uncertainty_can_be_switched_off(monkeypatch, tmp_path):
    """Disabled returns exactly the payload shape that predates intervals."""
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_long_dataset_dir(tmp_path))
    payload = {**WF_LONG_PAYLOAD, "uncertainty": {"enabled": False}}
    body = client.post("/walkforward", json=payload).json()

    for fold in body["folds"]:
        assert "uncertainty" not in fold["test_metrics"]
    # The pooled point estimate survives; only the interval is withheld.
    assert "uncertainty" not in body["oos_metrics"]


def test_the_seed_makes_an_interval_reproducible(monkeypatch, tmp_path):
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_long_dataset_dir(tmp_path))
    first = client.post("/walkforward", json=WF_LONG_PAYLOAD).json()
    again = client.post("/walkforward", json=WF_LONG_PAYLOAD).json()
    assert (
        first["oos_metrics"]["uncertainty"]["sharpe_ratio"]
        == again["oos_metrics"]["uncertainty"]["sharpe_ratio"]
    )

    moved = client.post("/walkforward", json={**WF_LONG_PAYLOAD, "uncertainty": {"seed": 7}}).json()
    assert (
        moved["oos_metrics"]["uncertainty"]["sharpe_ratio"]
        != first["oos_metrics"]["uncertainty"]["sharpe_ratio"]
    )


def test_a_fold_too_short_to_bootstrap_still_reports_its_metrics(monkeypatch, tmp_path):
    """An interval is an enrichment, not a precondition.

    The shared 30-bar fixture gives 4-return test slices, below the bootstrap's
    minimum. Those folds must still come back with a Sharpe rather than erroring
    -- and the pooled path, being the concatenation of all of them, can clear the
    minimum even when no single fold does. That asymmetry is the case for pooling
    in miniature.
    """
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    body = client.post("/walkforward", json=WF_PAYLOAD).json()

    for fold in body["folds"]:
        assert "uncertainty" not in fold["test_metrics"], "4 returns is too few to bound"
        assert "sharpe_ratio" in fold["test_metrics"], "but the estimate must survive"


def test_sweep_responses_carry_no_intervals(monkeypatch, tmp_path):
    """The sweep grid stays bare, deliberately.

    A per-cell interval would cost cells x resamples, and a grid of independent
    intervals invites reading a selected maximum as though the selection were
    free -- which is what the Deflated Sharpe Ratio exists to correct, and not
    something an interval can fix.
    """
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    body = client.post("/sweep", json=SWEEP_PAYLOAD).json()
    assert body["results"]
    for point in body["results"]:
        assert "uncertainty" not in point["metrics"]


def test_walkforward_unknown_dataset_returns_404(monkeypatch, tmp_path):
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    payload = {**WF_PAYLOAD, "dataset": "NOSUCHFILE.csv"}
    assert client.post("/walkforward", json=payload).status_code == 404


def test_walkforward_invalid_n_windows_returns_422(monkeypatch, tmp_path):
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    payload = {**WF_PAYLOAD, "n_windows": 1}
    assert client.post("/walkforward", json=payload).status_code == 422


def test_walkforward_fill_timing_accepted(monkeypatch, tmp_path):
    pytest.importorskip("backtesting_py")
    monkeypatch.setenv("RUSTY_FINANCE_DATA_DIR", _seed_dataset_dir(tmp_path))
    for timing in ("close", "next_open"):
        payload = {**WF_PAYLOAD, "fill_timing": timing}
        assert client.post("/walkforward", json=payload).status_code == 200
