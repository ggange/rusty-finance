import csv
import json
import os
from pathlib import Path
from typing import Annotated, Literal, Optional, Union

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


# ─── Portfolio request (multi-asset) ─────────────────────────────────────────

class DatasetSource(BaseModel):
    """Reference a CSV in the server-side data catalog by file name."""
    kind: Literal["dataset"] = "dataset"
    name: str


class InlineSource(BaseModel):
    """Carry candles inline (e.g. a CSV the user uploaded in the browser)."""
    kind: Literal["inline"] = "inline"
    candles: list[CandleIn] = Field(min_length=1)


AssetSource = Annotated[
    Union[DatasetSource, InlineSource],
    Field(discriminator="kind"),
]


class AssetIn(BaseModel):
    symbol: str = Field(min_length=1)
    weight: Optional[float] = Field(default=None, gt=0)
    source: AssetSource
    strategy: StrategyParams


class PortfolioRequest(BaseModel):
    assets: list[AssetIn] = Field(min_length=1)
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


# ─── Dataset catalog ──────────────────────────────────────────────────────────

def _data_dir() -> Path:
    """Directory holding one CSV per symbol. Override with RUSTY_FINANCE_DATA_DIR;
    defaults to <repo>/data/datasets."""
    env = os.environ.get("RUSTY_FINANCE_DATA_DIR")
    if env:
        return Path(env)
    return Path(__file__).resolve().parent.parent / "data" / "datasets"


def _read_csv_candles(path: Path) -> list[dict]:
    """Parse a CSV into candle dicts, tolerating both upper- and lowercase headers."""
    def pick(row: dict, *keys: str):
        for k in keys:
            if row.get(k) not in (None, ""):
                return row[k]
        return None

    with path.open(newline="") as f:
        out = []
        for row in csv.DictReader(f):
            vol = pick(row, "Volume", "volume")
            out.append({
                "date": pick(row, "Date", "date"),
                "open": float(pick(row, "Open", "open")),
                "high": float(pick(row, "High", "high")),
                "low": float(pick(row, "Low", "low")),
                "close": float(pick(row, "Close", "close")),
                "volume": int(float(vol)) if vol is not None else 0,
            })
        return out


def _list_datasets() -> list[dict]:
    """Enumerate available CSV datasets with light metadata. Missing dir → []."""
    data_dir = _data_dir()
    if not data_dir.is_dir():
        return []
    datasets = []
    for path in sorted(data_dir.glob("*.csv")):
        try:
            candles = _read_csv_candles(path)
        except Exception:
            continue  # skip unparseable files rather than failing the whole list
        if not candles:
            continue
        datasets.append({
            "name": path.name,
            "symbol": path.stem,
            "rows": len(candles),
            "start": candles[0]["date"],
            "end": candles[-1]["date"],
        })
    return datasets


def _load_dataset(name: str) -> list[dict]:
    """Load a named dataset's candles, guarding against path traversal."""
    safe = os.path.basename(name)  # strip any directory components
    if safe != name or not safe:
        raise HTTPException(status_code=422, detail=f"invalid dataset name: {name!r}")
    path = _data_dir() / safe
    if not path.is_file():
        raise HTTPException(status_code=404, detail=f"dataset not found: {name!r}")
    return _read_csv_candles(path)


def _resolve_asset_candles(asset: AssetIn) -> list[dict]:
    """Resolve an asset's data source to a list of candle dicts."""
    src = asset.source
    if isinstance(src, DatasetSource):
        return _load_dataset(src.name)
    return [c.model_dump() for c in src.candles]


def _portfolio_json(req: PortfolioRequest) -> str:
    """Assemble the JSON the Rust engine expects: assets with inline candles."""
    assets = []
    for a in req.assets:
        entry = {
            "symbol": a.symbol,
            "strategy": json.loads(a.strategy.model_dump_json()),
            "candles": _resolve_asset_candles(a),
        }
        if a.weight is not None:
            entry["weight"] = a.weight
        assets.append(entry)
    return json.dumps(assets)


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


@app.get("/datasets")
def list_datasets():
    """List CSV datasets available on the server (default <repo>/data/datasets).

    Each asset in a portfolio can reference one of these by name, or supply
    inline candles instead.
    """
    return {"datasets": _list_datasets()}


@app.get("/datasets/{name}")
def get_dataset(name: str):
    """Return the candles for one named dataset.

    Used by the UI to draw the per-asset candlestick chart when an asset is
    sourced from the server catalog rather than an uploaded file.
    """
    return {"name": name, "candles": _load_dataset(name)}


@app.post("/portfolio")
def portfolio(req: PortfolioRequest):
    """Run a multi-asset portfolio backtest.

    Capital is split across assets by (normalized) weight; each asset runs its
    own strategy. Returns the aggregate equity curve, portfolio metrics, a
    weighted buy-and-hold benchmark, and a per-asset breakdown.
    """
    _require_engine()
    raw = bt.run_portfolio(
        _portfolio_json(req),
        req.initial_cash,
        req.commission,
        req.slippage_pct,
    )
    return json.loads(raw)
