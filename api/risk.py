"""Pre-trade risk guardrails and the manual kill switch.

Roadmap principle 6: capital safety beats every feature. Nothing may reach a
broker without passing through `evaluate`, which is the single chokepoint the
trading loop calls before every submission.

Two deliberate asymmetries, both in the direction of safety:

* **Guardrails constrain entries, never exits.** A limit exists to stop the
  system taking on *more* risk. Blocking a sell would strand capital in a
  position the strategy wants out of, so risk-reducing orders always pass.
* **The kill switch halts everything, including exits.** "Kill" means the
  automation stops touching the account entirely and a human takes over —
  a halt that still traded would not be a halt. This is the one case where an
  exit can be blocked, and it is always a deliberate human action.

Limits resolve per-plan first, then fall back to the global row. A limit of
None means unlimited, which is also what an absent row means.

**Fail-closed for live brokers.** An unconfigured install is permissive, which
is fine while nothing can move money. The moment a broker reports `is_live`,
that inverts: `require_limits=True` makes a missing capital-safety limit a
rejection rather than a licence. `max_position_value` and `max_daily_loss` are
both required — one bounds a single mistake, the other bounds a bad day.
`max_daily_orders` is a runaway-loop guard, not a capital limit, so it stays
optional.
"""

from dataclasses import dataclass

LIMIT_FIELDS = ("max_position_value", "max_daily_loss", "max_daily_orders")

# Limits that must be configured before a live broker may be used at all.
REQUIRED_FOR_LIVE = ("max_position_value", "max_daily_loss")


@dataclass(frozen=True)
class Decision:
    """Outcome of a pre-trade check."""

    allowed: bool
    reason: str | None = None

    @property
    def status(self) -> str:
        """Status string recorded on the intent row."""
        return "allowed" if self.allowed else f"rejected: {self.reason}"


def resolve_limits(plan_limits: dict | None, global_limits: dict | None) -> dict:
    """Merge per-plan limits over global ones, field by field.

    A plan that sets only max_daily_loss still inherits the global position cap.
    """
    resolved = {}
    for field in LIMIT_FIELDS:
        value = (plan_limits or {}).get(field)
        if value is None:
            value = (global_limits or {}).get(field)
        resolved[field] = value
    return resolved


def evaluate(
    side: str,
    qty: float,
    price: float,
    limits: dict,
    stats: dict,
    kill_switch: dict | None = None,
    require_limits: bool = False,
) -> Decision:
    """Decide whether one order may be submitted.

    `stats` is the plan's activity so far today: {"orders", "realized_pnl"}.
    `require_limits` should be True whenever the target broker is live.
    Returns a Decision; the caller records the reason on the intent either way,
    so a rejection is auditable rather than silent.
    """
    if kill_switch and kill_switch.get("engaged"):
        note = kill_switch.get("reason")
        return Decision(False, f"kill switch engaged{f': {note}' if note else ''}")

    # Fail-closed: a live broker with no configured limits is refused outright,
    # in both directions. This is the one check that also blocks exits, because
    # an unconfigured live system should not be trading at all.
    if require_limits:
        missing = [f for f in REQUIRED_FOR_LIVE if limits.get(f) is None]
        if missing:
            return Decision(
                False,
                f"live broker requires risk limits; not configured: {', '.join(missing)}",
            )

    # Exits are risk-reducing and are never blocked by a limit.
    if side == "sell":
        return Decision(True)

    max_position_value = limits.get("max_position_value")
    if max_position_value is not None:
        notional = qty * price
        if notional > max_position_value:
            return Decision(
                False,
                f"position value {notional:,.2f} exceeds max_position_value "
                f"{max_position_value:,.2f}",
            )

    max_daily_orders = limits.get("max_daily_orders")
    if max_daily_orders is not None and stats.get("orders", 0) >= max_daily_orders:
        return Decision(
            False,
            f"daily order count {stats['orders']} has reached max_daily_orders "
            f"{max_daily_orders}",
        )

    max_daily_loss = limits.get("max_daily_loss")
    if max_daily_loss is not None:
        pnl = stats.get("realized_pnl", 0.0)
        if pnl <= -abs(max_daily_loss):
            return Decision(
                False,
                f"realized loss {pnl:,.2f} today has reached max_daily_loss "
                f"{abs(max_daily_loss):,.2f}",
            )

    return Decision(True)
