# api

FastAPI server that exposes the Rust backtesting engine over HTTP.

## Setup

```bash
# 1. Build and install the Rust extension into your active virtualenv
cd ../backtesting-py && maturin develop

# 2. Install API dependencies
cd ../api && pip install -e ".[dev]"
```

## Running the Server

```bash
uvicorn api.main:app --reload
# Server starts at http://localhost:8000
# Interactive docs: http://localhost:8000/docs
```

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Engine availability check |
| POST | `/backtest/ma` | Run a Moving Average Crossover backtest |
| POST | `/backtest/rsi` | Run an RSI backtest |

### POST /backtest/ma

```json
{
  "csv_path": "/path/to/prices.csv",
  "short_window": 5,
  "long_window": 20,
  "initial_cash": 10000.0
}
```

### POST /backtest/rsi

```json
{
  "csv_path": "/path/to/prices.csv",
  "period": 14,
  "initial_cash": 10000.0
}
```

### Response shape (both endpoints)

```json
{
  "equity_curve": [{"date": "2024-01-02", "nav": 10000.0}, ...],
  "trades": [{"date": "...", "action": "Buy", "shares": 99, "price": 100.0, "cash_after": 100.0}],
  "metrics": {"total_return": 0.05, "max_drawdown": -0.12, "sharpe_ratio": 1.3}
}
```

Returns `503` if `backtesting_py` is not installed. Returns `422` for invalid request bodies.

## Running Tests

```bash
pytest tests/
```

Tests that require the native extension are auto-skipped if it is not installed.
