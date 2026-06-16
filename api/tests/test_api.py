import json
import os

import pytest
from fastapi.testclient import TestClient

import api.main
from api.main import app

client = TestClient(app, raise_server_exceptions=False)

FIXTURE = os.path.join(os.path.dirname(__file__), "../../data/fixtures/synthetic_30.csv")


# ---------------------------------------------------------------------------
# Health endpoint
# ---------------------------------------------------------------------------

def test_health_returns_200():
    response = client.get("/health")
    assert response.status_code == 200
    assert "status" in response.json()
    assert response.json()["status"] == "ok"


def test_health_engine_field_present():
    response = client.get("/health")
    body = response.json()
    assert "engine" in body
    assert body["engine"] in ("available", "unavailable")


# ---------------------------------------------------------------------------
# 503 when engine unavailable (monkeypatched)
# ---------------------------------------------------------------------------

def test_ma_returns_503_when_engine_unavailable(monkeypatch):
    monkeypatch.setattr(api.main, "_ENGINE_AVAILABLE", False)
    response = client.post("/backtest/ma", json={"csv_path": "/some/path.csv"})
    assert response.status_code == 503
    assert "not installed" in response.json()["detail"]


def test_rsi_returns_503_when_engine_unavailable(monkeypatch):
    monkeypatch.setattr(api.main, "_ENGINE_AVAILABLE", False)
    response = client.post("/backtest/rsi", json={"csv_path": "/some/path.csv"})
    assert response.status_code == 503


# ---------------------------------------------------------------------------
# Validation errors (engine may or may not be available)
# ---------------------------------------------------------------------------

def test_ma_missing_fields_returns_422():
    response = client.post("/backtest/ma", json={})
    assert response.status_code == 422


def test_rsi_missing_fields_returns_422():
    response = client.post("/backtest/rsi", json={})
    assert response.status_code == 422


# ---------------------------------------------------------------------------
# Engine-dependent tests (auto-skipped if backtesting_py not installed)
# ---------------------------------------------------------------------------

def test_ma_returns_200_with_valid_fixture():
    pytest.importorskip("backtesting_py")
    response = client.post("/backtest/ma", json={
        "csv_path": FIXTURE,
        "short_window": 3,
        "long_window": 5,
        "initial_cash": 10000.0,
    })
    assert response.status_code == 200
    body = response.json()
    assert "equity_curve" in body
    assert "trades" in body
    assert "metrics" in body
    assert isinstance(body["metrics"]["total_return"], float)


def test_rsi_returns_200_with_valid_fixture():
    pytest.importorskip("backtesting_py")
    response = client.post("/backtest/rsi", json={
        "csv_path": FIXTURE,
        "period": 7,
        "initial_cash": 10000.0,
    })
    assert response.status_code == 200
    body = response.json()
    assert "equity_curve" in body
    assert "trades" in body
    assert "metrics" in body


def test_ma_nonexistent_csv_causes_error():
    pytest.importorskip("backtesting_py")
    response = client.post("/backtest/ma", json={"csv_path": "/tmp/does_not_exist.csv"})
    assert response.status_code in (500, 422)
