import json
from typing import Optional
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

try:
    import backtesting_py as bt
    _ENGINE_AVAILABLE = True
except ImportError:
    _ENGINE_AVAILABLE = False

app = FastAPI(title="Rusty Finance API", version="0.2.0")


# ─── Request / response models ────────────────────────────────────────────────

class CandleIn(BaseModel):
    date: str
    open: float
    high: float
    low: float
    close: float
    volume: int = 0


class ExecutionParams(BaseModel):
    initial_cash: float = Field(default=10_000.0, gt=0)
    commission: float = Field(default=0.0, ge=0)
    slippage_pct: float = Field(default=0.0, ge=0, lt=1)


class MARequest(ExecutionParams):
    candles: list[CandleIn]
    short_window: int = Field(default=5, gt=0)
    long_window: int = Field(default=20, gt=0)


class RSIRequest(ExecutionParams):
    candles: list[CandleIn]
    period: int = Field(default=14, gt=1)


# ─── Helpers ──────────────────────────────────────────────────────────────────

def _require_engine() -> None:
    if not _ENGINE_AVAILABLE:
        raise HTTPException(
            status_code=503,
            detail="backtesting_py not installed — run: cd backtesting-py && maturin develop",
        )


def _candles_json(candles: list[CandleIn]) -> str:
    return json.dumps([c.model_dump() for c in candles])


# ─── Endpoints ────────────────────────────────────────────────────────────────

@app.get("/health")
def health():
    return {"status": "ok", "engine": "available" if _ENGINE_AVAILABLE else "unavailable"}


@app.post("/backtest/ma")
def backtest_ma(req: MARequest):
    _require_engine()
    raw = bt.run_ma(
        _candles_json(req.candles),
        req.short_window,
        req.long_window,
        req.initial_cash,
        req.commission,
        req.slippage_pct,
    )
    return json.loads(raw)


@app.post("/backtest/rsi")
def backtest_rsi(req: RSIRequest):
    _require_engine()
    raw = bt.run_rsi(
        _candles_json(req.candles),
        req.period,
        req.initial_cash,
        req.commission,
        req.slippage_pct,
    )
    return json.loads(raw)
