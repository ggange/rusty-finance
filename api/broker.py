"""Broker abstraction: order intents in, order state out.

The loop never talks to a venue directly — it goes through a `Broker`. Adapters
differ in what they can do to real money, so every broker declares `is_live`,
and the risk layer refuses to submit through a live one until limits are
configured (see `api.risk.evaluate`).

Order state is deliberately richer than a status string. Real venues accept an
order and then fill it over time, partially or not at all, so `BrokerOrder`
carries `filled_qty` and `avg_fill_price` and can be re-polled via `get_order`.
`list_positions` is what reconciliation compares our ledger against.
"""

import logging
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import datetime, timezone
from itertools import count

logger = logging.getLogger(__name__)

# Terminal states — an order in one of these will never change again.
TERMINAL_STATUSES = frozenset({"filled", "rejected", "canceled"})
OPEN_STATUSES = frozenset({"accepted", "partially_filled"})


def _now() -> str:
    return datetime.now(timezone.utc).isoformat()


def apply_fill(
    positions: dict[str, dict], symbol: str, side: str, qty: float, price: float
) -> None:
    """Fold a fill into an in-memory position map, weighting the average price.

    Shared by the simulated brokers so they model holdings the same way; a real
    adapter reports positions from the venue instead of computing them.
    """
    pos = positions.setdefault(symbol, {"symbol": symbol, "qty": 0.0, "avg_price": 0.0})
    if side == "buy":
        total = pos["qty"] + qty
        if total > 0:
            pos["avg_price"] = (pos["qty"] * pos["avg_price"] + qty * price) / total
        pos["qty"] = total
    else:
        pos["qty"] = max(0.0, pos["qty"] - qty)
        if pos["qty"] == 0:
            pos["avg_price"] = 0.0


@dataclass
class OrderIntent:
    symbol: str
    side: str  # "buy" | "sell"
    qty: float
    price: float
    reason: str
    strategy: str
    ts: str = field(default_factory=_now)


@dataclass
class BrokerOrder:
    """State of an order at the venue.

    `filled_qty` / `avg_fill_price` describe what actually executed, which is
    what the position ledger must be built from — never the requested qty.
    """

    broker_order_id: str
    status: str  # accepted | partially_filled | filled | rejected | canceled
    filled_qty: float = 0.0
    avg_fill_price: float = 0.0
    reason: str | None = None
    ts: str = field(default_factory=_now)

    @property
    def is_terminal(self) -> bool:
        return self.status in TERMINAL_STATUSES

    def to_dict(self) -> dict:
        return {
            "broker_order_id": self.broker_order_id,
            "status": self.status,
            "filled_qty": self.filled_qty,
            "avg_fill_price": self.avg_fill_price,
            "reason": self.reason,
            "ts": self.ts,
        }


class Broker(ABC):
    """A venue adapter.

    Subclasses MUST set `is_live` truthfully: it gates the fail-closed risk
    check, so understating it defeats the guardrails.
    """

    name: str = "broker"
    is_live: bool = False

    @abstractmethod
    async def submit(self, intent: OrderIntent) -> BrokerOrder:
        """Submit an order intent and return its initial state at the venue."""

    @abstractmethod
    async def get_order(self, broker_order_id: str) -> BrokerOrder | None:
        """Re-poll a previously submitted order. None if the venue doesn't know it."""

    @abstractmethod
    async def list_positions(self) -> list[dict]:
        """Venue-reported holdings as [{symbol, qty, avg_price}] — for reconciliation."""


class DryRunBroker(Broker):
    """Logs intents and fills them instantly and completely. Touches no money.

    It keeps an internal position map so reconciliation is exercised against a
    real second source of truth rather than trivially agreeing with the ledger.
    """

    name = "dry_run"
    is_live = False

    def __init__(self) -> None:
        self._orders: dict[str, BrokerOrder] = {}
        self._positions: dict[str, dict] = {}
        self._ids = count(1)

    async def submit(self, intent: OrderIntent) -> BrokerOrder:
        logger.info(
            "DRY-RUN %s %s qty=%.4f price=%.4f reason=%r strategy=%s",
            intent.side.upper(), intent.symbol, intent.qty, intent.price,
            intent.reason, intent.strategy,
        )
        order = BrokerOrder(
            broker_order_id=f"dry-{next(self._ids)}",
            status="filled",
            filled_qty=intent.qty,
            avg_fill_price=intent.price,
        )
        self._orders[order.broker_order_id] = order
        apply_fill(self._positions, intent.symbol, intent.side, intent.qty, intent.price)
        return order

    async def get_order(self, broker_order_id: str) -> BrokerOrder | None:
        return self._orders.get(broker_order_id)

    async def list_positions(self) -> list[dict]:
        return [p for p in self._positions.values() if p["qty"] != 0]


class SimulatedPaperBroker(Broker):
    """A configurable stand-in that models the messy parts of a real venue.

    Exists so the order lifecycle — partial fills, rejections, orders that stay
    open across ticks — can be exercised before any vendor is chosen. Still not
    live: `is_live` is False and no money moves.

    `fill_ratio` < 1 produces a partial fill; `reject_symbols` rejects outright.
    """

    name = "simulated_paper"
    is_live = False

    def __init__(
        self,
        fill_ratio: float = 1.0,
        reject_symbols: frozenset[str] | set[str] | None = None,
        slippage: float = 0.0,
    ) -> None:
        self.fill_ratio = fill_ratio
        self.reject_symbols = set(reject_symbols or ())
        self.slippage = slippage
        self._orders: dict[str, BrokerOrder] = {}
        self._positions: dict[str, dict] = {}
        self._ids = count(1)

    async def submit(self, intent: OrderIntent) -> BrokerOrder:
        oid = f"sim-{next(self._ids)}"

        if intent.symbol in self.reject_symbols:
            order = BrokerOrder(oid, "rejected", reason="symbol rejected by venue")
            self._orders[oid] = order
            return order

        filled = intent.qty * self.fill_ratio
        # Buys slip up, sells slip down — always against us.
        direction = 1 if intent.side == "buy" else -1
        price = intent.price * (1 + direction * self.slippage)

        if filled <= 0:
            status = "accepted"
        elif filled < intent.qty:
            status = "partially_filled"
        else:
            status = "filled"

        order = BrokerOrder(oid, status, filled_qty=filled, avg_fill_price=price if filled else 0.0)
        self._orders[oid] = order
        if filled > 0:
            apply_fill(self._positions, intent.symbol, intent.side, filled, price)
        return order

    async def get_order(self, broker_order_id: str) -> BrokerOrder | None:
        return self._orders.get(broker_order_id)

    async def list_positions(self) -> list[dict]:
        return [p for p in self._positions.values() if p["qty"] != 0]
