import csv
import itertools
import json
import os
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Annotated, Literal, Optional, Union

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field, model_validator

import api.db as db

try:
    import backtesting_py as bt
    _ENGINE_AVAILABLE = True
except ImportError:
    _ENGINE_AVAILABLE = False

@asynccontextmanager
async def lifespan(application: FastAPI):
    await db.init_db()
    yield


app = FastAPI(
    title="Rusty Finance API",
    version="0.3.0",
    description="Backtesting engine powered by a native Rust core.",
    lifespan=lifespan,
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


class MACDParams(BaseModel):
    type: Literal["macd"] = "macd"
    fast_period: int = Field(default=12, ge=2, description="Fast EMA period (bars)")
    slow_period: int = Field(default=26, ge=3, description="Slow EMA period (bars)")
    signal_period: int = Field(default=9, ge=2, description="Signal line EMA period (bars)")

    @model_validator(mode="after")
    def periods_ordered(self) -> "MACDParams":
        if self.fast_period >= self.slow_period:
            raise ValueError("fast_period must be less than slow_period")
        return self


class BollingerBandsParams(BaseModel):
    type: Literal["bollinger_bands"] = "bollinger_bands"
    period: int = Field(default=20, ge=2, description="Lookback period (bars)")
    std_dev_mult: float = Field(default=2.0, ge=0.5, le=5.0, description="Band width in standard deviations")


StrategyParams = Annotated[
    Union[MAEmaParams, MASmaParams, MAWmaParams, RSIParams, MACDParams, BollingerBandsParams],
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
    benchmark_symbol: Optional[str] = Field(default=None, description="Dataset name to use as external benchmark (e.g. 'SPY.csv')")


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
    {
        "type": "macd",
        "name": "MACD",
        "description": "Buy when MACD line crosses above signal line; sell on cross below.",
        "params": [
            {"name": "fast_period",   "type": "integer", "default": 12, "min": 2, "description": "Fast EMA period (bars)"},
            {"name": "slow_period",   "type": "integer", "default": 26, "min": 3, "description": "Slow EMA period (bars)"},
            {"name": "signal_period", "type": "integer", "default": 9,  "min": 2, "description": "Signal line period (bars)"},
        ],
    },
    {
        "type": "bollinger_bands",
        "name": "Bollinger Bands",
        "description": "Buy when price closes below lower band; sell when it closes above upper band.",
        "params": [
            {"name": "period",       "type": "integer", "default": 20,  "min": 2,   "description": "Lookback period (bars)"},
            {"name": "std_dev_mult", "type": "number",  "default": 2.0, "min": 0.5, "step": 0.5, "description": "Band width (σ)"},
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


def _compute_bah_curve(candles: list[dict], initial_cash: float) -> list[dict]:
    """Buy-and-hold NAV series: fractional shares of initial_cash at first bar's close."""
    if not candles:
        return []
    start_price = candles[0]["close"]
    if start_price <= 0:
        return []
    shares = initial_cash / start_price
    return [{"date": c["date"], "nav": shares * c["close"]} for c in candles]


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
async def backtest(req: BacktestRequest):
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
    result = json.loads(raw)
    run_id = await db.save_run("backtest", req.model_dump(mode="json"), result)
    result["run_id"] = run_id
    return result


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
async def portfolio(req: PortfolioRequest):
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
    result = json.loads(raw)
    if req.benchmark_symbol:
        bench_candles = _load_dataset(req.benchmark_symbol)
        result["external_benchmark_curve"] = _compute_bah_curve(bench_candles, req.initial_cash)
    run_id = await db.save_run("portfolio", req.model_dump(mode="json"), result)
    result["run_id"] = run_id
    return result


# ─── Sweep request ────────────────────────────────────────────────────────────

class ParamRange(BaseModel):
    """One parameter's sweep range: start at `min`, step by `step`, up to `max`."""
    min: float
    max: float
    step: float = Field(gt=0)


class SweepRequest(BaseModel):
    dataset: str
    strategy_type: str
    param_ranges: dict[str, ParamRange] = Field(min_length=1)
    initial_cash: float = Field(default=10_000.0, gt=0)
    commission: float = Field(default=0.0, ge=0)
    slippage_pct: float = Field(default=0.0, ge=0, lt=1)


def _expand_param_grid(strategy_type: str, param_ranges: dict[str, "ParamRange"]) -> list[dict]:
    """Expand per-parameter ranges into the flat Cartesian product of valid strategy specs."""
    range_lists: dict[str, list[float]] = {}
    for name, rng in param_ranges.items():
        vals: list[float] = []
        v = rng.min
        while v <= rng.max + rng.step * 1e-6:
            vals.append(round(v, 8))
            v += rng.step
        range_lists[name] = vals

    keys = list(range_lists.keys())
    combos = list(itertools.product(*[range_lists[k] for k in keys]))

    valid: list[dict] = []
    for combo in combos:
        params = {keys[i]: combo[i] for i in range(len(keys))}
        # Coerce whole-number floats to int so Rust's usize fields deserialize correctly.
        coerced = {k: (int(v) if isinstance(v, float) and v == int(v) else v) for k, v in params.items()}
        spec = {"type": strategy_type, **coerced}
        try:
            # Validate via Pydantic — skip combos that violate strategy constraints.
            _validate_strategy_spec(spec)
            valid.append(spec)
        except Exception:
            continue
    return valid


def _validate_strategy_spec(spec: dict) -> None:
    """Raise if the strategy spec violates constraints (e.g. short_window >= long_window)."""
    from pydantic import TypeAdapter
    TypeAdapter(StrategyParams).validate_python(spec)


@app.post("/sweep")
async def sweep(req: SweepRequest):
    """Run one strategy over a grid of parameter combinations on a single dataset.

    Returns a list of `{ params, metrics }` objects — one per valid combination.
    Combinations that violate strategy constraints (e.g. short_window >= long_window)
    are silently skipped.
    """
    _require_engine()
    candles = _load_dataset(req.dataset)
    grid = _expand_param_grid(req.strategy_type, req.param_ranges)
    if not grid:
        raise HTTPException(status_code=422, detail="No valid parameter combinations in the given ranges")

    raw = bt.run_sweep(
        json.dumps(grid),
        json.dumps(candles),
        req.initial_cash,
        req.commission,
        req.slippage_pct,
    )
    return {"results": json.loads(raw)}


@app.get("/runs")
async def list_runs(limit: int = 50):
    """List recent backtest and portfolio runs, most recent first."""
    return {"runs": await db.list_runs(limit)}


@app.get("/runs/{run_id}")
async def get_run(run_id: int):
    """Retrieve the full config and result for a saved run."""
    run = await db.get_run(run_id)
    if run is None:
        raise HTTPException(status_code=404, detail="Run not found")
    return run
