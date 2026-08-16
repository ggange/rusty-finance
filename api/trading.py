import json
from dataclasses import dataclass
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


class EngineUnavailable(RuntimeError):
    """The Rust bindings aren't importable, so no signal can be computed."""


def _require_engine() -> None:
    """Fail loudly and legibly when the bindings are missing.

    The HTTP layer guards its own endpoints, but the scheduler calls `run_tick`
    directly — without this, a missing binding surfaces as a bare
    `NameError: name 'bt' is not defined` inside a 16:30 cron log.
    """
    if not _ENGINE_AVAILABLE:
        raise EngineUnavailable(
            "backtesting_py not installed — run: cd backtesting-py && maturin develop"
        )


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


def realized_pnl(intent: OrderIntent, position: dict | None, order=None) -> float | None:
    """Realized PnL for an exit, or None for an entry (which realizes nothing).

    Computed from the *filled* quantity and price when an order is supplied, so
    a partial exit realizes only the part that actually sold.
    """
    if intent.side != "sell" or not position:
        return None
    qty = order.filled_qty if order is not None else intent.qty
    price = order.avg_fill_price if order is not None and order.filled_qty else intent.price
    if qty <= 0:
        return None
    return (price - position["avg_price"]) * qty


async def apply_fill_to_ledger(
    plan_id: str,
    symbol: str,
    strategy_key: str,
    side: str,
    position: dict | None,
    order,
) -> None:
    """Move the position ledger by what the venue actually filled.

    A rejected or unfilled order leaves the ledger exactly as it was; a partial
    buy records only the filled shares, and a partial sell leaves the remainder
    still held.
    """
    if order.filled_qty <= 0:
        return

    held = position["qty"] if position else 0.0

    if side == "buy":
        total = held + order.filled_qty
        prior_cost = held * (position["avg_price"] if position else 0.0)
        avg = (prior_cost + order.filled_qty * order.avg_fill_price) / total
        await db.upsert_position(plan_id, symbol, strategy_key, total, avg)
    else:
        remaining = max(0.0, held - order.filled_qty)
        avg = position["avg_price"] if (position and remaining > 0) else 0.0
        await db.upsert_position(plan_id, symbol, strategy_key, remaining, avg)


async def sync_open_orders(plan_id: str, broker) -> list[dict]:
    """Re-poll orders the venue hasn't finished with and fold in any new fills.

    Without this, a partially filled order would stay partial in our records
    forever even after the venue completed it.
    """
    synced = []
    for row in await db.list_open_orders(plan_id):
        if not row["broker_order_id"]:
            continue
        latest = await broker.get_order(row["broker_order_id"])
        if latest is None or (
            latest.status == row["status"] and latest.filled_qty == row["filled_qty"]
        ):
            continue

        newly_filled = latest.filled_qty - row["filled_qty"]
        await db.update_order_state(
            row["id"], latest.status, latest.filled_qty, latest.avg_fill_price, latest.reason
        )
        if newly_filled > 0:
            position = await db.get_position(plan_id, row["symbol"], row["strategy"])
            delta = SimpleFill(newly_filled, latest.avg_fill_price)
            await apply_fill_to_ledger(
                plan_id, row["symbol"], row["strategy"], row["side"], position, delta
            )
        synced.append({
            "order_id": row["id"],
            "symbol": row["symbol"],
            "from": row["status"],
            "to": latest.status,
            "newly_filled": newly_filled,
        })
    return synced


@dataclass
class SimpleFill:
    """Just the incremental part of a fill, shaped for apply_fill_to_ledger."""

    filled_qty: float
    avg_fill_price: float


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
    _require_engine()
    results = []

    limits = await resolve_plan_limits(plan_id)
    kill_switch = await db.get_kill_switch()
    today = datetime.now(timezone.utc).date().isoformat()

    # Catch up on anything the venue finished since the last tick before
    # deciding anything new — otherwise we'd decide against a stale position.
    synced = await sync_open_orders(plan_id, broker)

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
                require_limits=broker.is_live,
            )

            order = await broker.submit(intent) if decision.allowed else None
            status = order.status if order else decision.status

            # The ledger follows what actually *executed*, never what we asked
            # for — a partial fill leaves a partial position.
            pnl = realized_pnl(intent, position, order) if order else None

            intent_id = await db.record_intent(
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

            if order is not None:
                await db.record_order(
                    plan_id=plan_id,
                    symbol=symbol,
                    strategy=strategy_key,
                    side=intent.side,
                    qty=intent.qty,
                    broker=broker.name,
                    status=order.status,
                    broker_order_id=order.broker_order_id,
                    filled_qty=order.filled_qty,
                    avg_fill_price=order.avg_fill_price,
                    reason=order.reason,
                    intent_id=intent_id,
                )
                await apply_fill_to_ledger(
                    plan_id, symbol, strategy_key, intent.side, position, order
                )

            intent_dict = {
                "side": intent.side,
                "qty": intent.qty,
                "price": intent.price,
                "reason": intent.reason,
                "status": status,
                "allowed": decision.allowed,
                "rejected_reason": decision.reason,
                "realized_pnl": pnl,
                "order": order.to_dict() if order else None,
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
        "synced_orders": synced,
        "reconciliation": await reconcile(plan_id, broker),
    }


async def soak_report(plan_id: str | None = None) -> dict:
    """Execution statistics from the trading loop: fill rate, rejections, PnL,
    and the gap between `signal_price` and `avg_fill_price` in basis points.

    Read the slippage figure with two caveats, both of which mean it is *not*
    the engine validating itself:

    1. **Against a simulated broker it is circular.** `PersistentPaperBroker`
       and `SimulatedPaperBroker` fill at `intent.price × (1 + slippage)`, and
       this function measures the gap from `intent.price` — so it recovers the
       configured slippage constant by construction and tells you nothing about
       fill realism. Only a real venue makes this number informative.
    2. **`signal_price` is the last bar's close, not the price the engine
       assumes.** The default `FillTiming` is `next_open`, so the backtester
       assumes a fill at the *next* bar's open. The bps figure below therefore
       excludes the overnight gap, which for a loop that fires after the close
       is the largest component of the real modelling error. Comparing against
       the next open requires a bar that does not exist yet at signal time; it
       has to be reconciled on the following tick.

    What this report does measure honestly: fill rate, rejection count and
    realized PnL. Principle 5's comparison — do live fills match what the
    backtest assumed — is not yet implemented, and cannot be against a broker
    whose fills we define.
    """
    orders = await db.list_orders(plan_id=plan_id, limit=100_000)
    venue = {o["broker_order_id"]: o for o in await db.venue_orders_all()}

    total = len(orders)
    rejected = [o for o in orders if o["status"] == "rejected"]
    filled = [o for o in orders if o["filled_qty"] > 0]

    slippages_bps = []
    for order in filled:
        v = venue.get(order["broker_order_id"])
        signal_price = (v or {}).get("signal_price")
        if not signal_price or not order["avg_fill_price"]:
            continue
        # Positive = worse than the backtest assumed, for both sides.
        direction = 1 if order["side"] == "buy" else -1
        bps = direction * (order["avg_fill_price"] - signal_price) / signal_price * 10_000
        slippages_bps.append(bps)

    requested = sum(o["qty"] for o in orders if o["status"] != "rejected")
    got = sum(o["filled_qty"] for o in orders)

    mean_slip = sum(slippages_bps) / len(slippages_bps) if slippages_bps else None
    worst_slip = max(slippages_bps) if slippages_bps else None

    return {
        "plan_id": plan_id,
        "orders": total,
        "filled": len(filled),
        "rejected": len(rejected),
        "fill_rate": (got / requested) if requested else None,
        "slippage_bps": {
            "samples": len(slippages_bps),
            "mean": mean_slip,
            "worst": worst_slip,
            "note": "positive = execution worse than the backtest assumed",
        },
        "realized_pnl": await db.total_realized_pnl(plan_id),
    }


async def reconcile(plan_id: str, broker, tolerance: float = 1e-6) -> dict:
    """Compare venue-reported holdings against our own ledger, per symbol.

    Drift means our record of reality is wrong, which makes every subsequent
    decision suspect — so it's reported on every tick rather than on request
    only. Symbols held on one side and not the other are drift too, not
    omissions, so the comparison runs over the union of both sets.
    """
    ours = await db.ledger_by_symbol(plan_id)
    theirs = {p["symbol"]: float(p["qty"]) for p in await broker.list_positions()}

    drift = []
    for symbol in sorted(set(ours) | set(theirs)):
        our_qty = ours.get(symbol, 0.0)
        their_qty = theirs.get(symbol, 0.0)
        delta = their_qty - our_qty
        if abs(delta) > tolerance:
            drift.append({
                "symbol": symbol,
                "ledger_qty": our_qty,
                "broker_qty": their_qty,
                "delta": delta,
            })

    return {
        "broker": broker.name,
        "in_sync": not drift,
        "checked": len(set(ours) | set(theirs)),
        "drift": drift,
    }
