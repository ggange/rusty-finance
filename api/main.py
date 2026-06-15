import json
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

# Import native Rust bindings (install with: cd backtesting-py && maturin develop)
try:
    import backtesting_py as bt
    _ENGINE_AVAILABLE = True
except ImportError:
    _ENGINE_AVAILABLE = False

app = FastAPI(title="Rusty Finance API", version="0.1.0")


class MARequest(BaseModel):
    csv_path: str
    short_window: int = 5
    long_window: int = 20
    initial_cash: float = 10_000.0


class RSIRequest(BaseModel):
    csv_path: str
    period: int = 14
    initial_cash: float = 10_000.0


def _require_engine():
    if not _ENGINE_AVAILABLE:
        raise HTTPException(
            status_code=503,
            detail="backtesting_py not installed — run: cd backtesting-py && maturin develop",
        )


@app.get("/health")
def health():
    return {"status": "ok", "engine": "available" if _ENGINE_AVAILABLE else "unavailable"}


@app.post("/backtest/ma")
def backtest_ma(req: MARequest):
    _require_engine()
    raw = bt.run_ma(req.csv_path, req.short_window, req.long_window, req.initial_cash)
    return json.loads(raw)


@app.post("/backtest/rsi")
def backtest_rsi(req: RSIRequest):
    _require_engine()
    raw = bt.run_rsi(req.csv_path, req.period, req.initial_cash)
    return json.loads(raw)
