import json
from datetime import datetime, timezone
from pathlib import Path

import api.db as db
from api import risk
from api.broker import Broker, OrderIntent
from api.datasets import load_dataset, candles_to_json

try:
    import backtesting_py as bt
    _ENGINE_AVAILABLE = True
except ImportError:
    _ENGINE_AVAILABLE = False


def resolve_items(items: list[dict]) -> list[dict]:
    """Expand stored/requested plan items into the shape run_tick consumes.

    Input items are {dataset, strategy (dict), cash_allocation}; this loads each
    dataset's candles and derives the symbol and the canonical strategy_key.
    Shared by the /trade/tick endpoint and the scheduler so a scheduled tick and
    a manual one address exactly the same ledger rows.
    """
    resolved = []
    for item in items:
        dataset = item["dataset"]
        strategy = item["strategy"]
        resolved.append({
            "symbol": dataset.removesuffix(".csv").upper(),
            "strategy_json": json.dumps(strategy),
            "strategy_key": json.dumps(strategy, sort_keys=True),
            "cash_allocation": item["cash_allocation"],
            "candles": load_dataset(dataset),
        })
    return resolved


def decide(
    signal: str,
    position: dict | None,
    latest_close: float,
    cash_allocation: float,
    symbol: str,
    strategy_key: str,
) -> OrderIntent | None:
    """Pure decision function: map (signal, current position) to an OrderIntent or None."""
    qty = position["qty"] if position else 0.0

    if signal == "buy":
        if qty > 0:
            return None  # already long — idempotent no-op
        return OrderIntent(
            symbol=symbol,
            side="buy",
            qty=cash_allocation / latest_close,
            price=latest_close,
            reason="entry: buy signal, flat",
            strategy=strategy_key,
        )

    if signal == "sell":
        if qty <= 0:
            return None  # already flat — nothing to sell
        return OrderIntent(
            symbol=symbol,
            side="sell",
            qty=qty,
            price=latest_close,
            reason="exit: sell signal",
            strategy=strategy_key,
        )

    return None  # hold


def realized_pnl(intent: OrderIntent, position: dict | None) -> float | None:
    """Realized PnL for an exit, or None for an entry (which realizes nothing)."""
    if intent.side != "sell" or not position:
        return None
    return (intent.price - position["avg_price"]) * intent.qty


async def resolve_plan_limits(plan_id: str) -> dict:
    """Effective limits for a plan: its own row layered over the global row."""
    return risk.resolve_limits(
        await db.get_limits_row(plan_id),
        await db.get_limits_row(db.GLOBAL_LIMITS),
    )


async def run_tick(plan_id: str, items: list[dict], broker: Broker) -> dict:
    """
    Execute one tick of the trading loop.

    Each item must have: dataset (str), strategy_json (str), strategy_key (str),
    symbol (str), cash_allocation (float).

    Every order passes through the risk chokepoint (`api.risk.evaluate`) before
    it can reach the broker. Rejected orders are still written to the intent log
    — with a "rejected: ..." status and no position change — so a blocked trade
    is auditable rather than invisible.

    Returns { plan_id, results: [...], positions: [...], limits, kill_switch }.
    """
    results = []

    limits = await resolve_plan_limits(plan_id)
    kill_switch = await db.get_kill_switch()
    today = datetime.now(timezone.utc).date().isoformat()

    for item in items:
        symbol = item["symbol"]
        strategy_key = item["strategy_key"]
        strategy_json = item["strategy_json"]
        cash_allocation = item["cash_allocation"]
        candles = item["candles"]

        candles_json = candles_to_json(candles)
        raw = bt.latest_signal(strategy_json, candles_json)
        latest = json.loads(raw)
        signal = latest["signal"]
        close = latest["close"]

        position = await db.get_position(plan_id, symbol, strategy_key)
        intent = decide(signal, position, close, cash_allocation, symbol, strategy_key)

        intent_dict = None
        if intent is not None:
            # ── Risk chokepoint: nothing reaches the broker without passing here ──
            stats = await db.daily_stats(plan_id, today)
            decision = risk.evaluate(
                side=intent.side,
                qty=intent.qty,
                price=intent.price,
                limits=limits,
                stats=stats,
                kill_switch=kill_switch,
            )

            pnl = realized_pnl(intent, position) if decision.allowed else None
            status = await broker.submit(intent) if decision.allowed else decision.status

            await db.record_intent(
                plan_id=plan_id,
                symbol=symbol,
                strategy=strategy_key,
                side=intent.side,
                qty=intent.qty,
                price=intent.price,
                signal=signal,
                reason=intent.reason,
                status=status,
                realized_pnl=pnl,
            )

            # A rejected order must leave the ledger untouched — the position is
            # whatever it was before we tried.
            if decision.allowed:
                if intent.side == "buy":
                    await db.upsert_position(plan_id, symbol, strategy_key, intent.qty, intent.price)
                else:
                    await db.upsert_position(plan_id, symbol, strategy_key, 0.0, 0.0)

            intent_dict = {
                "side": intent.side,
                "qty": intent.qty,
                "price": intent.price,
                "reason": intent.reason,
                "status": status,
                "allowed": decision.allowed,
                "rejected_reason": decision.reason,
                "realized_pnl": pnl,
            }

        results.append({
            "symbol": symbol,
            "signal": signal,
            "date": latest["date"],
            "close": close,
            "intent": intent_dict,
        })

    positions = await db.list_positions(plan_id)
    return {
        "plan_id": plan_id,
        "results": results,
        "positions": positions,
        "limits": limits,
        "kill_switch": kill_switch,
    }
