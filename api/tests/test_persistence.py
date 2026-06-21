import os
import csv

import pytest
from fastapi.testclient import TestClient

from api.main import app

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

PORTFOLIO_PAYLOAD = {
    "assets": [
        {
            "symbol": "A",
            "weight": 1.0,
            "source": {"kind": "inline", "candles": CANDLES},
            "strategy": {"type": "ma_sma", "short_window": 3, "long_window": 5},
        }
    ],
    "initial_cash": 10_000.0,
}


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture()
def client(monkeypatch, tmp_path):
    """A fresh TestClient backed by an isolated SQLite DB for each test.

    Using TestClient as a context manager runs the FastAPI lifespan (init_db)
    after the env var is patched, ensuring the table is created in tmp_path.
    """
    db_file = str(tmp_path / "test_runs.db")
    monkeypatch.setenv("RUSTY_FINANCE_DB", db_file)
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def test_backtest_saves_run(client):
    pytest.importorskip("backtesting_py")
    resp = client.post("/backtest", json=MA_PAYLOAD)
    assert resp.status_code == 200
    body = resp.json()
    assert "run_id" in body
    run_id = body["run_id"]

    runs_resp = client.get("/runs")
    assert runs_resp.status_code == 200
    runs = runs_resp.json()["runs"]
    assert len(runs) == 1
    assert runs[0]["id"] == run_id
    assert runs[0]["kind"] == "backtest"


def test_get_run_returns_full_result(client):
    pytest.importorskip("backtesting_py")
    post_resp = client.post("/backtest", json=MA_PAYLOAD)
    assert post_resp.status_code == 200
    run_id = post_resp.json()["run_id"]

    get_resp = client.get(f"/runs/{run_id}")
    assert get_resp.status_code == 200
    run = get_resp.json()
    assert run["id"] == run_id
    assert run["kind"] == "backtest"
    assert "config" in run
    assert "result" in run
    assert all(k in run["result"] for k in ("equity_curve", "trades", "metrics", "benchmark"))


def test_get_unknown_run_returns_404(client):
    resp = client.get("/runs/99999")
    assert resp.status_code == 404


def test_portfolio_saves_run(client):
    pytest.importorskip("backtesting_py")
    resp = client.post("/portfolio", json=PORTFOLIO_PAYLOAD)
    assert resp.status_code == 200
    body = resp.json()
    assert "run_id" in body

    runs = client.get("/runs").json()["runs"]
    assert len(runs) == 1
    assert runs[0]["kind"] == "portfolio"


def test_runs_ordered_most_recent_first(client):
    pytest.importorskip("backtesting_py")
    for _ in range(3):
        client.post("/backtest", json=MA_PAYLOAD)
    runs = client.get("/runs").json()["runs"]
    assert len(runs) == 3
    ids = [r["id"] for r in runs]
    assert ids == sorted(ids, reverse=True)
