import json
from pathlib import Path

import api.db as db
from api.broker import Broker, OrderIntent
from api.datasets import load_dataset, candles_to_json

try:
    import backtesting_py as bt
    _ENGINE_AVAILABLE = True
except ImportError:
    _ENGINE_AVAILABLE = False


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


async def run_tick(plan_id: str, items: list[dict], broker: Broker) -> dict:
    """
    Execute one tick of the trading loop.

    Each item must have: dataset (str), strategy_json (str), strategy_key (str),
    symbol (str), cash_allocation (float).

    Returns { plan_id, results: [...], positions: [...] }.
    """
    results = []

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

        status = None
        intent_dict = None
        if intent is not None:
            status = await broker.submit(intent)
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
            )
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
            }

        results.append({
            "symbol": symbol,
            "signal": signal,
            "date": latest["date"],
            "close": close,
            "intent": intent_dict,
        })

    positions = await db.list_positions(plan_id)
    return {"plan_id": plan_id, "results": results, "positions": positions}
