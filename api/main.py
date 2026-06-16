import json
from typing import Annotated, Literal, Union

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field, model_validator

try:
    import backtesting_py as bt
    _ENGINE_AVAILABLE = True
except ImportError:
    _ENGINE_AVAILABLE = False

app = FastAPI(
    title="Rusty Finance API",
    version="0.3.0",
    description="Backtesting engine powered by a native Rust core.",
)


# ─── Candle ───────────────────────────────────────────────────────────────────

class CandleIn(BaseModel):
    date: str
    open: float
    high: float
    low: float
    close: float
    volume: int = 0


# ─── Strategy parameter models (discriminated union) ─────────────────────────

class MAEmaParams(BaseModel):
    type: Literal["ma_ema"] = "ma_ema"
    short_window: int = Field(default=5, gt=0, description="Short EMA period (bars)")
    long_window: int = Field(default=20, gt=0, description="Long EMA period (bars)")

    @model_validator(mode="after")
    def windows_ordered(self) -> "MAEmaParams":
        if self.short_window >= self.long_window:
            raise ValueError("short_window must be less than long_window")
        return self


class MASmaParams(BaseModel):
    type: Literal["ma_sma"] = "ma_sma"
    short_window: int = Field(default=5, gt=0, description="Short SMA period (bars)")
    long_window: int = Field(default=20, gt=0, description="Long SMA period (bars)")

    @model_validator(mode="after")
    def windows_ordered(self) -> "MASmaParams":
        if self.short_window >= self.long_window:
            raise ValueError("short_window must be less than long_window")
        return self


class MAWmaParams(BaseModel):
    type: Literal["ma_wma"] = "ma_wma"
    short_window: int = Field(default=5, gt=0, description="Short WMA period (bars)")
    long_window: int = Field(default=20, gt=0, description="Long WMA period (bars)")

    @model_validator(mode="after")
    def windows_ordered(self) -> "MAWmaParams":
        if self.short_window >= self.long_window:
            raise ValueError("short_window must be less than long_window")
        return self


class RSIParams(BaseModel):
    type: Literal["rsi"] = "rsi"
    period: int = Field(default=14, gt=1, description="RSI look-back period (bars)")


StrategyParams = Annotated[
    Union[MAEmaParams, MASmaParams, MAWmaParams, RSIParams],
    Field(discriminator="type"),
]


# ─── Backtest request ─────────────────────────────────────────────────────────

class BacktestRequest(BaseModel):
    strategy: StrategyParams
    candles: list[CandleIn] = Field(min_length=1)
    initial_cash: float = Field(default=10_000.0, gt=0)
    commission: float = Field(default=0.0, ge=0)
    slippage_pct: float = Field(default=0.0, ge=0, lt=1)


# ─── Strategy registry metadata (consumed by the UI) ─────────────────────────

_REGISTRY = [
    {
        "type": "ma_ema",
        "name": "EMA Crossover",
        "description": "Buy when short EMA crosses above long EMA; sell on cross below.",
        "params": [
            {"name": "short_window", "type": "integer", "default": 5,  "min": 1, "description": "Short EMA period (bars)"},
            {"name": "long_window",  "type": "integer", "default": 20, "min": 2, "description": "Long EMA period (bars)"},
        ],
    },
    {
        "type": "ma_sma",
        "name": "SMA Crossover",
        "description": "Buy when short SMA crosses above long SMA; sell on cross below.",
        "params": [
            {"name": "short_window", "type": "integer", "default": 5,  "min": 1},
            {"name": "long_window",  "type": "integer", "default": 20, "min": 2},
        ],
    },
    {
        "type": "ma_wma",
        "name": "WMA Crossover",
        "description": "Buy when short WMA crosses above long WMA; sell on cross below.",
        "params": [
            {"name": "short_window", "type": "integer", "default": 5,  "min": 1},
            {"name": "long_window",  "type": "integer", "default": 20, "min": 2},
        ],
    },
    {
        "type": "rsi",
        "name": "RSI",
        "description": "Buy when RSI < 30 (oversold); sell when RSI > 70 (overbought).",
        "params": [
            {"name": "period", "type": "integer", "default": 14, "min": 2, "description": "Look-back period (bars)"},
        ],
    },
]


# ─── Helpers ──────────────────────────────────────────────────────────────────

def _require_engine() -> None:
    if not _ENGINE_AVAILABLE:
        raise HTTPException(
            status_code=503,
            detail="backtesting_py not installed — run: cd backtesting-py && maturin develop",
        )


def _candles_json(candles: list[CandleIn]) -> str:
    return json.dumps([c.model_dump() for c in candles])


def _strategy_json(strategy: StrategyParams) -> str:
    return strategy.model_dump_json()


# ─── Endpoints ────────────────────────────────────────────────────────────────

@app.get("/health")
def health():
    return {"status": "ok", "engine": "available" if _ENGINE_AVAILABLE else "unavailable"}


@app.get("/strategies")
def list_strategies():
    """Return the registry of available strategies and their parameter schemas.

    The UI uses this to dynamically render strategy-picker forms.
    """
    return {"strategies": _REGISTRY}


@app.post("/backtest")
def backtest(req: BacktestRequest):
    """Run a backtest for any registered strategy.

    The `strategy.type` discriminator selects the strategy; remaining fields
    in `strategy` are its parameters.
    """
    _require_engine()
    raw = bt.run(
        _strategy_json(req.strategy),
        _candles_json(req.candles),
        req.initial_cash,
        req.commission,
        req.slippage_pct,
    )
    return json.loads(raw)
