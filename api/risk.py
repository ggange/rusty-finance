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
None means unlimited, which is also what an absent row means — so a fresh
install is permissive by default and becomes safe the moment limits are set.
Callers that need a floor should set global limits at deploy time.
"""

from dataclasses import dataclass

LIMIT_FIELDS = ("max_position_value", "max_daily_loss", "max_daily_orders")


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
) -> Decision:
    """Decide whether one order may be submitted.

    `stats` is the plan's activity so far today: {"orders", "realized_pnl"}.
    Returns a Decision; the caller records the reason on the intent either way,
    so a rejection is auditable rather than silent.
    """
    if kill_switch and kill_switch.get("engaged"):
        note = kill_switch.get("reason")
        return Decision(False, f"kill switch engaged{f': {note}' if note else ''}")

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
