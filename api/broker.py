import logging
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import datetime, timezone

logger = logging.getLogger(__name__)


@dataclass
class OrderIntent:
    symbol: str
    side: str  # "buy" | "sell"
    qty: float
    price: float
    reason: str
    strategy: str
    ts: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())


class Broker(ABC):
    @abstractmethod
    async def submit(self, intent: OrderIntent) -> str:
        """Submit an order intent. Returns a status string."""


class DryRunBroker(Broker):
    async def submit(self, intent: OrderIntent) -> str:
        logger.info(
            "DRY-RUN %s %s qty=%.4f price=%.4f reason=%r strategy=%s",
            intent.side.upper(),
            intent.symbol,
            intent.qty,
            intent.price,
            intent.reason,
            intent.strategy,
        )
        return "dry_run"
