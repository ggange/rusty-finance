import itertools
import json
from contextlib import asynccontextmanager
from typing import Annotated, Literal, Optional, Union

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field, model_validator

import api.db as db
from api.datasets import load_dataset, list_datasets as _list_datasets_impl, candles_to_json

try:
    import backtesting_py as bt
    _ENGINE_AVAILABLE = True
except ImportError:
    _ENGINE_AVAILABLE = False

@asynccontextmanager
async def lifespan(application: FastAPI):
    import logging

    from api import scheduler as sched

    await db.init_db()

    # An unconfigured install is permissive (no limits == unlimited). That is
    # tolerable while the only broker is DryRunBroker, but it must not be the
    # state anything live ever starts in — so say so on every boot.
    if await db.get_limits_row(db.GLOBAL_LIMITS) is None:
        logging.getLogger("api.risk").warning(
            "no global risk limits set — orders are unbounded. "
            "POST /trade/limits before connecting a real broker."
        )
    if (await db.get_kill_switch())["engaged"]:
        logging.getLogger("api.risk").warning(
            "kill switch is ENGAGED — no orders will be submitted."
        )

    sched.start_scheduler()
    try:
        yield
    finally:
        sched.shutdown_scheduler()


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
    fill_timing: Literal["close", "next_open"] = Field(
        default="next_open",
        description="close = fill at same bar's close (legacy); next_open = fill at next bar's open (realistic)",
    )


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


class RebalanceFrequencyMonthly(BaseModel):
    kind: Literal["monthly"] = "monthly"


class RebalanceFrequencyQuarterly(BaseModel):
    kind: Literal["quarterly"] = "quarterly"


class RebalanceFrequencyThreshold(BaseModel):
    kind: Literal["threshold"] = "threshold"
    threshold: float = Field(gt=0, lt=1, description="Drift threshold as a fraction (e.g. 0.05 = 5 %)")


RebalanceFrequency = Annotated[
    Union[RebalanceFrequencyMonthly, RebalanceFrequencyQuarterly, RebalanceFrequencyThreshold],
    Field(discriminator="kind"),
]


class RebalanceConfigIn(BaseModel):
    frequency: RebalanceFrequency


class PortfolioRequest(BaseModel):
    assets: list[AssetIn] = Field(min_length=1)
    initial_cash: float = Field(default=10_000.0, gt=0)
    commission: float = Field(default=0.0, ge=0)
    slippage_pct: float = Field(default=0.0, ge=0, lt=1)
    benchmark_symbol: Optional[str] = Field(default=None, description="Dataset name to use as external benchmark (e.g. 'SPY.csv')")
    rebalance: Optional[RebalanceConfigIn] = Field(default=None, description="Optional periodic or threshold rebalancing")
    fill_timing: Literal["close", "next_open"] = Field(
        default="next_open",
        description="close = fill at same bar's close (legacy); next_open = fill at next bar's open (realistic)",
    )


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
    return candles_to_json([c.model_dump() for c in candles])


def _strategy_json(strategy: StrategyParams) -> str:
    return strategy.model_dump_json()


# ─── Dataset catalog ──────────────────────────────────────────────────────────
# Implemented in api.datasets; aliases kept for backward-compat within this module.

def _list_datasets() -> list[dict]:
    return _list_datasets_impl()


def _load_dataset(name: str) -> list[dict]:
    return load_dataset(name)


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
    """Assemble the JSON the Rust engine expects: top-level envelope with assets + optional rebalance."""
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
    envelope: dict = {"assets": assets}
    if req.rebalance is not None:
        envelope["rebalance"] = {"frequency": json.loads(req.rebalance.frequency.model_dump_json())}
    return json.dumps(envelope)


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
        req.fill_timing,
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
        req.fill_timing,
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
    fill_timing: Literal["close", "next_open"] = Field(
        default="next_open",
        description="close = fill at same bar's close (legacy); next_open = fill at next bar's open (realistic)",
    )


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
        req.fill_timing,
    )
    return {"results": json.loads(raw)}


# ─── Walk-forward request ─────────────────────────────────────────────────────

class WalkForwardRequest(BaseModel):
    dataset: str
    strategy_type: str
    param_ranges: dict[str, ParamRange] = Field(min_length=1)
    n_windows: int = Field(default=5, ge=2, description="Number of rolling folds (>= 2)")
    train_frac: float = Field(default=0.7, gt=0.0, lt=1.0, description="Fraction of each fold used for training")
    metric: str = Field(default="sharpe_ratio", description="Metric to optimise on the train slice")
    initial_cash: float = Field(default=10_000.0, gt=0)
    commission: float = Field(default=0.0, ge=0)
    slippage_pct: float = Field(default=0.0, ge=0, lt=1)
    fill_timing: Literal["close", "next_open"] = Field(
        default="next_open",
        description="close = fill at same bar's close (legacy); next_open = fill at next bar's open (realistic)",
    )


@app.post("/walkforward")
async def walkforward(req: WalkForwardRequest):
    """Run walk-forward validation for a strategy over a single dataset.

    Splits the dataset into `n_windows` rolling folds. For each fold the best
    parameter combination (by `metric`) is selected on the train slice and then
    evaluated on the held-out test slice. Returns one result per fold.
    """
    _require_engine()
    candles = _load_dataset(req.dataset)
    grid = _expand_param_grid(req.strategy_type, req.param_ranges)
    if not grid:
        raise HTTPException(status_code=422, detail="No valid parameter combinations in the given ranges")

    raw = bt.run_walk_forward(
        json.dumps(grid),
        json.dumps(candles),
        req.n_windows,
        req.train_frac,
        req.metric,
        req.initial_cash,
        req.commission,
        req.slippage_pct,
        req.fill_timing,
    )
    return json.loads(raw)


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


# ─── Trading (dry-run loop) ───────────────────────────────────────────────────

class TradePlanItem(BaseModel):
    dataset: str
    strategy: StrategyParams
    cash_allocation: float = Field(gt=0, description="Cash to allocate to this position")


class TradeTickRequest(BaseModel):
    plan_id: str = Field(default="default", description="Logical plan identifier for ledger isolation")
    items: Optional[list[TradePlanItem]] = Field(
        default=None,
        description="Items to tick. Omit to use the stored plan with this plan_id.",
    )


class TradePlanRequest(BaseModel):
    plan_id: str = Field(default="default", description="Logical plan identifier")
    items: list[TradePlanItem] = Field(min_length=1)
    enabled: bool = Field(default=True, description="Whether the scheduler should run this plan")


def _plan_items_to_dicts(items: list[TradePlanItem]) -> list[dict]:
    """Normalise Pydantic plan items to the plain dicts stored and resolved elsewhere."""
    return [
        {
            "dataset": item.dataset,
            "strategy": json.loads(item.strategy.model_dump_json()),
            "cash_allocation": item.cash_allocation,
        }
        for item in items
    ]


@app.post("/trade/tick")
async def trade_tick(req: TradeTickRequest):
    """Run one tick of the dry-run trading loop.

    For each plan item: loads the dataset, asks the strategy for its latest
    signal, checks the position ledger for existing exposure, and emits an
    order intent (BUY / SELL / nothing) via the configured broker. All intents are
    logged to the order_intents table and positions are updated accordingly.
    Re-posting with the same state is idempotent (no duplicate BUY while long).

    With `items` omitted, the stored plan for `plan_id` is used — the same path
    the scheduler takes.
    """
    from api.broker import make_broker
    from api import trading

    _require_engine()

    if req.items is not None:
        items = _plan_items_to_dicts(req.items)
    else:
        stored = await db.get_plan(req.plan_id)
        if stored is None:
            raise HTTPException(
                status_code=404,
                detail=f"no stored plan {req.plan_id!r} — pass items or create the plan first",
            )
        items = stored["items"]

    return await trading.run_tick(req.plan_id, trading.resolve_items(items), make_broker())


# ─── Trading plans (what the scheduler runs) ──────────────────────────────────

@app.post("/trade/plans")
async def create_trade_plan(req: TradePlanRequest):
    """Create or replace a stored trading plan.

    Datasets are validated eagerly so a plan can't be saved pointing at data
    that doesn't exist — the scheduler runs unattended and shouldn't discover
    that at 16:30.
    """
    for item in req.items:
        _load_dataset(item.dataset)
    return await db.upsert_plan(req.plan_id, _plan_items_to_dicts(req.items), req.enabled)


@app.get("/trade/plans")
async def list_trade_plans(enabled_only: bool = False):
    return {"plans": await db.list_plans(enabled_only=enabled_only)}


@app.delete("/trade/plans/{plan_id}")
async def delete_trade_plan(plan_id: str):
    if not await db.delete_plan(plan_id):
        raise HTTPException(status_code=404, detail=f"plan not found: {plan_id!r}")
    return {"deleted": plan_id}


# ─── Risk guardrails & kill switch ────────────────────────────────────────────

class RiskLimitsRequest(BaseModel):
    plan_id: str = Field(
        default=db.GLOBAL_LIMITS,
        description="Plan these limits apply to; the default sets the global fallback",
    )
    max_position_value: Optional[float] = Field(
        default=None, gt=0, description="Max cash value of a single position (null = unlimited)"
    )
    max_daily_loss: Optional[float] = Field(
        default=None, gt=0, description="Halt new entries once realized loss today reaches this"
    )
    max_daily_orders: Optional[int] = Field(
        default=None, gt=0, description="Max orders submitted per day (runaway-loop guard)"
    )


class KillSwitchRequest(BaseModel):
    engaged: bool = Field(description="True halts all order submission")
    reason: Optional[str] = Field(default=None, description="Why it was flipped, for the audit log")


@app.get("/trade/limits")
async def get_trade_limits(plan_id: Optional[str] = None):
    """List stored limits, or the *effective* limits for one plan.

    Effective limits layer the plan's own row over the global row, which is what
    the trading loop actually enforces.
    """
    if plan_id is None:
        return {"limits": await db.list_limits()}
    from api import trading

    return {
        "plan_id": plan_id,
        "effective": await trading.resolve_plan_limits(plan_id),
        "plan": await db.get_limits_row(plan_id),
        "global": await db.get_limits_row(db.GLOBAL_LIMITS),
    }


@app.post("/trade/limits")
async def set_trade_limits(req: RiskLimitsRequest):
    """Set risk limits for a plan (or globally). Omitted fields mean 'no limit'."""
    return await db.set_limits(
        req.plan_id,
        max_position_value=req.max_position_value,
        max_daily_loss=req.max_daily_loss,
        max_daily_orders=req.max_daily_orders,
    )


@app.delete("/trade/limits/{plan_id}")
async def delete_trade_limits(plan_id: str):
    if not await db.delete_limits(plan_id):
        raise HTTPException(status_code=404, detail=f"no limits stored for: {plan_id!r}")
    return {"deleted": plan_id}


@app.get("/trade/killswitch")
async def get_trade_killswitch():
    return await db.get_kill_switch()


@app.post("/trade/killswitch")
async def set_trade_killswitch(req: KillSwitchRequest):
    """Engage or release the manual halt.

    While engaged, no order of any side reaches the broker — including exits.
    The state is stored in SQLite, so it survives an API restart: a halted
    system stays halted until a human explicitly releases it.
    """
    return await db.set_kill_switch(req.engaged, req.reason)


# ─── Scheduler ────────────────────────────────────────────────────────────────

@app.get("/trade/schedule")
async def get_trade_schedule():
    """Scheduler status: cron config, next fire time, and the last run's summary."""
    from api import scheduler as sched

    return await sched.schedule_status()


@app.post("/trade/schedule/run")
async def run_trade_schedule(refresh: Optional[bool] = None):
    """Trigger the scheduled cycle immediately (refresh bars, then tick all enabled plans).

    Pass `refresh=false` to tick against data already on disk — useful for
    testing the loop without hitting the network.
    """
    from api import scheduler as sched

    _require_engine()
    return await sched.run_all_plans(refresh=refresh)


@app.get("/trade/intents")
async def trade_intents(plan_id: str | None = None, limit: int = 50):
    """List recorded order intents, most recent first."""
    return {"intents": await db.list_intents(plan_id=plan_id, limit=limit)}


@app.get("/trade/positions")
async def trade_positions(plan_id: str | None = None):
    """List current position ledger entries."""
    return {"positions": await db.list_positions(plan_id=plan_id)}


@app.get("/trade/orders")
async def trade_orders(plan_id: str | None = None, open_only: bool = False, limit: int = 50):
    """Submitted orders and their fill state, most recent first."""
    if open_only:
        return {"orders": await db.list_open_orders(plan_id=plan_id)}
    return {"orders": await db.list_orders(plan_id=plan_id, limit=limit)}


@app.get("/trade/broker")
async def trade_broker():
    """Which broker the loop is configured to use, and how it's tuned."""
    from api.broker import broker_config, make_broker

    broker = make_broker()
    return {**broker_config(), "name": broker.name, "is_live": broker.is_live}


@app.get("/trade/soak")
async def trade_soak(plan_id: str | None = None):
    """Paper-soak evidence: fill rate, rejections, and realized slippage in bps.

    The backtester assumes a fill at the signal price, so the gap to the actual
    fill price is the modelling error. Near-zero mean slippage with a high fill
    rate is the engine validating itself; a persistent adverse skew means the
    backtest is optimistic (principle 5).
    """
    from api import trading

    return await trading.soak_report(plan_id)


@app.get("/trade/reconcile")
async def trade_reconcile(plan_id: str = "default"):
    """Compare broker-reported holdings against our ledger for one plan.

    Drift means our record of reality is wrong, so every decision made from it
    is suspect. Also run automatically on every tick.

    Note: DryRunBroker holds its position map in process memory, so with that
    broker a fresh process reports an empty venue and drift is expected. Use
    RUSTY_FINANCE_BROKER=paper_sim for a venue whose books survive restart.
    """
    from api.broker import make_broker
    from api import trading

    return await trading.reconcile(plan_id, make_broker())
