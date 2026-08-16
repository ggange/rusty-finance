import json
import os
from datetime import datetime, timezone
from pathlib import Path

import aiosqlite

_CREATE_SQL = """
CREATE TABLE IF NOT EXISTS runs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT    NOT NULL,
    kind       TEXT    NOT NULL,
    config     TEXT    NOT NULL,
    result     TEXT    NOT NULL
);
CREATE TABLE IF NOT EXISTS positions (
    plan_id    TEXT NOT NULL,
    symbol     TEXT NOT NULL,
    strategy   TEXT NOT NULL,
    qty        REAL NOT NULL DEFAULT 0,
    avg_price  REAL NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plan_id, symbol, strategy)
);
CREATE TABLE IF NOT EXISTS trade_plans (
    plan_id    TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    items      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS order_intents (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,
    plan_id    TEXT NOT NULL,
    symbol     TEXT NOT NULL,
    strategy   TEXT NOT NULL,
    side       TEXT NOT NULL,
    qty        REAL NOT NULL,
    price      REAL NOT NULL,
    signal     TEXT NOT NULL,
    reason     TEXT NOT NULL,
    status     TEXT NOT NULL,
    -- Whether this intent was actually submitted to a venue. Distinguishes a
    -- pre-trade risk rejection (never left the process) from a venue rejection
    -- (did), which the `status` string alone cannot: both read "rejected...".
    reached_broker INTEGER NOT NULL DEFAULT 0
);
-- One row per PnL-realizing event, and the only thing `daily_stats` sums.
--
-- PnL cannot live on the intent row alone: an exit that fills partially at
-- submit and completes on a later tick realizes PnL twice, at two different
-- times, and `max_daily_loss` has to see both on the days they happened.
-- `order_intents.realized_pnl` is kept as a denormalized display value for the
-- submit-time portion; this table is the source of truth.
CREATE TABLE IF NOT EXISTS realized_pnl_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,
    plan_id    TEXT NOT NULL,
    symbol     TEXT NOT NULL,
    strategy   TEXT NOT NULL,
    qty        REAL NOT NULL,
    amount     REAL NOT NULL,
    source     TEXT NOT NULL,  -- 'submit' | 'topup'
    order_id   INTEGER
);
CREATE INDEX IF NOT EXISTS idx_pnl_events_day
    ON realized_pnl_events (plan_id, created_at);
CREATE TABLE IF NOT EXISTS orders (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    plan_id         TEXT NOT NULL,
    symbol          TEXT NOT NULL,
    strategy        TEXT NOT NULL,
    side            TEXT NOT NULL,
    qty             REAL NOT NULL,
    broker          TEXT NOT NULL,
    broker_order_id TEXT,
    status          TEXT NOT NULL,
    filled_qty      REAL NOT NULL DEFAULT 0,
    avg_fill_price  REAL NOT NULL DEFAULT 0,
    reason          TEXT,
    intent_id       INTEGER
);
CREATE INDEX IF NOT EXISTS idx_orders_open ON orders (plan_id, status);
CREATE TABLE IF NOT EXISTS venue_positions (
    broker     TEXT NOT NULL,
    symbol     TEXT NOT NULL,
    qty        REAL NOT NULL DEFAULT 0,
    avg_price  REAL NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (broker, symbol)
);
CREATE TABLE IF NOT EXISTS venue_orders (
    broker_order_id TEXT PRIMARY KEY,
    broker          TEXT NOT NULL,
    symbol          TEXT NOT NULL,
    side            TEXT NOT NULL,
    qty             REAL NOT NULL,
    status          TEXT NOT NULL,
    filled_qty      REAL NOT NULL DEFAULT 0,
    avg_fill_price  REAL NOT NULL DEFAULT 0,
    signal_price    REAL,
    reason          TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS risk_limits (
    plan_id            TEXT PRIMARY KEY,
    max_position_value REAL,
    max_daily_loss     REAL,
    max_daily_orders   INTEGER,
    updated_at         TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS kill_switch (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    engaged    INTEGER NOT NULL DEFAULT 0,
    reason     TEXT,
    updated_at TEXT NOT NULL
);
"""

# Columns added after the tables above first shipped. CREATE TABLE IF NOT EXISTS
# won't add these to a database that already exists, so they're applied
# explicitly on every init.
_MIGRATIONS = [
    ("order_intents", "realized_pnl", "ALTER TABLE order_intents ADD COLUMN realized_pnl REAL"),
    (
        "order_intents",
        "reached_broker",
        "ALTER TABLE order_intents ADD COLUMN reached_broker INTEGER NOT NULL DEFAULT 0",
    ),
]


def _db_path() -> Path:
    return Path(os.getenv("RUSTY_FINANCE_DB", "data/runs.db"))


async def init_db() -> None:
    path = _db_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    async with aiosqlite.connect(path) as conn:
        await conn.executescript(_CREATE_SQL)
        for table, column, ddl in _MIGRATIONS:
            cur = await conn.execute(f"PRAGMA table_info({table})")
            existing = {row[1] for row in await cur.fetchall()}
            if column not in existing:
                await conn.execute(ddl)
        await conn.commit()



async def save_run(kind: str, config: dict, result: dict) -> int:
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        cur = await db.execute(
            "INSERT INTO runs (created_at, kind, config, result) VALUES (?,?,?,?)",
            (ts, kind, json.dumps(config), json.dumps(result)),
        )
        await db.commit()
        return cur.lastrowid


async def list_runs(limit: int = 50) -> list[dict]:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT id, created_at, kind, config FROM runs ORDER BY id DESC LIMIT ?",
            (limit,),
        )
        rows = await cur.fetchall()
    return [
        {
            "id": r["id"],
            "created_at": r["created_at"],
            "kind": r["kind"],
            "config": json.loads(r["config"]),
        }
        for r in rows
    ]


async def get_run(run_id: int) -> dict | None:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute("SELECT * FROM runs WHERE id = ?", (run_id,))
        row = await cur.fetchone()
    if row is None:
        return None
    return {
        "id": row["id"],
        "created_at": row["created_at"],
        "kind": row["kind"],
        "config": json.loads(row["config"]),
        "result": json.loads(row["result"]),
    }


# ─── Trading plans ────────────────────────────────────────────────────────────
#
# The scheduler fires with no HTTP request behind it, so the items it should
# trade have to live somewhere durable rather than in a request body.

def _row_to_plan(row) -> dict:
    return {
        "plan_id": row["plan_id"],
        "created_at": row["created_at"],
        "updated_at": row["updated_at"],
        "enabled": bool(row["enabled"]),
        "items": json.loads(row["items"]),
    }


async def upsert_plan(plan_id: str, items: list[dict], enabled: bool = True) -> dict:
    """Create or replace a trading plan. created_at survives an update."""
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        await db.execute(
            """
            INSERT INTO trade_plans (plan_id, created_at, updated_at, enabled, items)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(plan_id) DO UPDATE SET
                updated_at=excluded.updated_at,
                enabled=excluded.enabled,
                items=excluded.items
            """,
            (plan_id, ts, ts, int(enabled), json.dumps(items)),
        )
        await db.commit()
    return await get_plan(plan_id)


async def get_plan(plan_id: str) -> dict | None:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute("SELECT * FROM trade_plans WHERE plan_id=?", (plan_id,))
        row = await cur.fetchone()
    return _row_to_plan(row) if row is not None else None


async def list_plans(enabled_only: bool = False) -> list[dict]:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        sql = "SELECT * FROM trade_plans"
        if enabled_only:
            sql += " WHERE enabled=1"
        sql += " ORDER BY plan_id"
        cur = await db.execute(sql)
        rows = await cur.fetchall()
    return [_row_to_plan(r) for r in rows]


async def delete_plan(plan_id: str) -> bool:
    """Delete a plan. Returns True if a row was removed."""
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        cur = await db.execute("DELETE FROM trade_plans WHERE plan_id=?", (plan_id,))
        await db.commit()
        return cur.rowcount > 0


async def last_run_of_kind(kind: str) -> dict | None:
    """Most recent entry in `runs` for a given kind — used for scheduler status."""
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT * FROM runs WHERE kind=? ORDER BY id DESC LIMIT 1", (kind,)
        )
        row = await cur.fetchone()
    if row is None:
        return None
    return {
        "id": row["id"],
        "created_at": row["created_at"],
        "kind": row["kind"],
        "config": json.loads(row["config"]),
        "result": json.loads(row["result"]),
    }


# ─── Order lifecycle ──────────────────────────────────────────────────────────
#
# An intent is what we decided to do; an order is what the venue was told and
# what it actually did. They're separate tables because a rejected intent never
# becomes an order, and an order can outlive the tick that created it.

OPEN_ORDER_STATUSES = ("accepted", "partially_filled")


async def record_order(
    plan_id: str,
    symbol: str,
    strategy: str,
    side: str,
    qty: float,
    broker: str,
    status: str,
    broker_order_id: str | None = None,
    filled_qty: float = 0.0,
    avg_fill_price: float = 0.0,
    reason: str | None = None,
    intent_id: int | None = None,
) -> int:
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        cur = await db.execute(
            """
            INSERT INTO orders
              (created_at, updated_at, plan_id, symbol, strategy, side, qty, broker,
               broker_order_id, status, filled_qty, avg_fill_price, reason, intent_id)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)
            """,
            (ts, ts, plan_id, symbol, strategy, side, qty, broker, broker_order_id,
             status, filled_qty, avg_fill_price, reason, intent_id),
        )
        await db.commit()
        return cur.lastrowid


async def update_order_state(
    order_id: int,
    status: str,
    filled_qty: float,
    avg_fill_price: float,
    reason: str | None = None,
) -> None:
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        await db.execute(
            """
            UPDATE orders
               SET status=?, filled_qty=?, avg_fill_price=?, reason=?, updated_at=?
             WHERE id=?
            """,
            (status, filled_qty, avg_fill_price, reason, ts, order_id),
        )
        await db.commit()


async def list_orders(plan_id: str | None = None, limit: int = 50) -> list[dict]:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        if plan_id is not None:
            cur = await db.execute(
                "SELECT * FROM orders WHERE plan_id=? ORDER BY id DESC LIMIT ?",
                (plan_id, limit),
            )
        else:
            cur = await db.execute("SELECT * FROM orders ORDER BY id DESC LIMIT ?", (limit,))
        return [dict(r) for r in await cur.fetchall()]


async def list_open_orders(plan_id: str | None = None) -> list[dict]:
    """Orders the venue hasn't finished with — these need re-polling each tick."""
    await init_db()
    placeholders = ",".join("?" * len(OPEN_ORDER_STATUSES))
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        if plan_id is not None:
            cur = await db.execute(
                f"SELECT * FROM orders WHERE status IN ({placeholders}) AND plan_id=? ORDER BY id",
                (*OPEN_ORDER_STATUSES, plan_id),
            )
        else:
            cur = await db.execute(
                f"SELECT * FROM orders WHERE status IN ({placeholders}) ORDER BY id",
                OPEN_ORDER_STATUSES,
            )
        return [dict(r) for r in await cur.fetchall()]


async def ledger_by_symbol(plan_id: str | None = None) -> dict[str, float]:
    """Our own net quantity per symbol, aggregated across strategies.

    Broker positions are per symbol, but our ledger is keyed per
    (plan, symbol, strategy) — reconciliation compares at the symbol level.
    """
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        if plan_id is not None:
            cur = await db.execute(
                "SELECT symbol, SUM(qty) FROM positions WHERE plan_id=? GROUP BY symbol",
                (plan_id,),
            )
        else:
            cur = await db.execute("SELECT symbol, SUM(qty) FROM positions GROUP BY symbol")
        return {row[0]: float(row[1] or 0.0) for row in await cur.fetchall()}


# ─── Simulated venue state ────────────────────────────────────────────────────
#
# The simulated broker's *own* books, kept separate from our ledger on purpose:
# reconciliation is only meaningful if the two are independently derived. A real
# adapter reads these from the venue instead.

async def venue_apply_fill(
    broker: str, symbol: str, side: str, qty: float, price: float
) -> None:
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT qty, avg_price FROM venue_positions WHERE broker=? AND symbol=?",
            (broker, symbol),
        )
        row = await cur.fetchone()
        held = float(row["qty"]) if row else 0.0
        avg = float(row["avg_price"]) if row else 0.0

        if side == "buy":
            total = held + qty
            avg = ((held * avg) + (qty * price)) / total if total > 0 else 0.0
            held = total
        else:
            held = max(0.0, held - qty)
            if held == 0:
                avg = 0.0

        await db.execute(
            """
            INSERT INTO venue_positions (broker, symbol, qty, avg_price, updated_at)
            VALUES (?,?,?,?,?)
            ON CONFLICT(broker, symbol) DO UPDATE SET
                qty=excluded.qty, avg_price=excluded.avg_price, updated_at=excluded.updated_at
            """,
            (broker, symbol, held, avg, ts),
        )
        await db.commit()


async def venue_positions(broker: str) -> list[dict]:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT symbol, qty, avg_price FROM venue_positions "
            "WHERE broker=? AND qty != 0 ORDER BY symbol",
            (broker,),
        )
        return [dict(r) for r in await cur.fetchall()]


async def venue_record_order(
    broker_order_id: str,
    broker: str,
    symbol: str,
    side: str,
    qty: float,
    status: str,
    filled_qty: float = 0.0,
    avg_fill_price: float = 0.0,
    signal_price: float | None = None,
    reason: str | None = None,
) -> None:
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        await db.execute(
            """
            INSERT INTO venue_orders
              (broker_order_id, broker, symbol, side, qty, status, filled_qty,
               avg_fill_price, signal_price, reason, created_at, updated_at)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?)
            ON CONFLICT(broker_order_id) DO UPDATE SET
                status=excluded.status, filled_qty=excluded.filled_qty,
                avg_fill_price=excluded.avg_fill_price, reason=excluded.reason,
                updated_at=excluded.updated_at
            """,
            (broker_order_id, broker, symbol, side, qty, status, filled_qty,
             avg_fill_price, signal_price, reason, ts, ts),
        )
        await db.commit()


async def venue_get_order(broker_order_id: str) -> dict | None:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT * FROM venue_orders WHERE broker_order_id=?", (broker_order_id,)
        )
        row = await cur.fetchone()
    return dict(row) if row else None


async def venue_orders_all(broker: str | None = None) -> list[dict]:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        if broker is not None:
            cur = await db.execute("SELECT * FROM venue_orders WHERE broker=?", (broker,))
        else:
            cur = await db.execute("SELECT * FROM venue_orders")
        return [dict(r) for r in await cur.fetchall()]


async def total_realized_pnl(plan_id: str | None = None) -> float:
    """Sum of realized PnL across all recorded exits.

    Reads `realized_pnl_events`, the same source `daily_stats` uses, so a partial
    exit that the venue completed on a later tick is counted in full. Summing
    `order_intents.realized_pnl` instead would report only the portion that
    filled at submit time.
    """
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        sql = "SELECT COALESCE(SUM(amount), 0) FROM realized_pnl_events"
        params: tuple = ()
        if plan_id is not None:
            sql += " WHERE plan_id=?"
            params = (plan_id,)
        cur = await db.execute(sql, params)
        return float((await cur.fetchone())[0] or 0.0)


async def venue_next_order_seq(broker: str) -> int:
    """Monotonic per-broker order counter that survives restart."""
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        cur = await db.execute("SELECT COUNT(*) FROM venue_orders WHERE broker=?", (broker,))
        return int((await cur.fetchone())[0]) + 1


# ─── Risk limits & kill switch ────────────────────────────────────────────────
#
# Limits are stored per plan, with the reserved plan_id GLOBAL_LIMITS acting as
# the fallback for plans that have none of their own.

GLOBAL_LIMITS = "__global__"


def _row_to_limits(row) -> dict:
    return {
        "plan_id": row["plan_id"],
        "max_position_value": row["max_position_value"],
        "max_daily_loss": row["max_daily_loss"],
        "max_daily_orders": row["max_daily_orders"],
        "updated_at": row["updated_at"],
    }


async def set_limits(
    plan_id: str,
    max_position_value: float | None = None,
    max_daily_loss: float | None = None,
    max_daily_orders: int | None = None,
) -> dict:
    """Set (replacing) the risk limits for a plan. None means 'no limit'."""
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        await db.execute(
            """
            INSERT INTO risk_limits
              (plan_id, max_position_value, max_daily_loss, max_daily_orders, updated_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(plan_id) DO UPDATE SET
                max_position_value=excluded.max_position_value,
                max_daily_loss=excluded.max_daily_loss,
                max_daily_orders=excluded.max_daily_orders,
                updated_at=excluded.updated_at
            """,
            (plan_id, max_position_value, max_daily_loss, max_daily_orders, ts),
        )
        await db.commit()
    return await get_limits_row(plan_id)


async def get_limits_row(plan_id: str) -> dict | None:
    """Raw limits stored for exactly this plan_id (no global fallback)."""
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute("SELECT * FROM risk_limits WHERE plan_id=?", (plan_id,))
        row = await cur.fetchone()
    return _row_to_limits(row) if row is not None else None


async def list_limits() -> list[dict]:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute("SELECT * FROM risk_limits ORDER BY plan_id")
        rows = await cur.fetchall()
    return [_row_to_limits(r) for r in rows]


async def delete_limits(plan_id: str) -> bool:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        cur = await db.execute("DELETE FROM risk_limits WHERE plan_id=?", (plan_id,))
        await db.commit()
        return cur.rowcount > 0


async def get_kill_switch() -> dict:
    """Current kill-switch state. Absent row means disengaged."""
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute("SELECT * FROM kill_switch WHERE id=1")
        row = await cur.fetchone()
    if row is None:
        return {"engaged": False, "reason": None, "updated_at": None}
    return {
        "engaged": bool(row["engaged"]),
        "reason": row["reason"],
        "updated_at": row["updated_at"],
    }


async def set_kill_switch(engaged: bool, reason: str | None = None) -> dict:
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        await db.execute(
            """
            INSERT INTO kill_switch (id, engaged, reason, updated_at)
            VALUES (1, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                engaged=excluded.engaged,
                reason=excluded.reason,
                updated_at=excluded.updated_at
            """,
            (int(engaged), reason, ts),
        )
        await db.commit()
    return await get_kill_switch()


async def daily_stats(plan_id: str, day: str) -> dict:
    """Orders submitted and realized PnL for a plan on a given UTC date (YYYY-MM-DD).

    Feeds `max_daily_orders` and `max_daily_loss`, so what it counts *is* what
    those guards can see.

    **Orders** counts intents that actually reached a venue, via the explicit
    `reached_broker` flag. Filtering on the status string instead cannot express
    this: a pre-trade risk rejection and a venue rejection both read "rejected",
    and only the first should be free. A pre-trade rejection sent nothing, so
    charging it would let one misconfigured limit lock a plan out; a venue
    rejection did send something, and a symbol the venue rejects every tick is
    precisely the runaway `max_daily_orders` exists to bound.

    **Realized PnL** sums `realized_pnl_events` rather than the intent rows,
    because an exit can realize PnL more than once — partially at submit, and
    again whenever the venue tops the fill up on a later tick.
    """
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        cur = await db.execute(
            """
            SELECT COUNT(*) FROM order_intents
            WHERE plan_id = ?
              AND substr(created_at, 1, 10) = ?
              AND reached_broker = 1
            """,
            (plan_id, day),
        )
        (count,) = await cur.fetchone()

        cur = await db.execute(
            """
            SELECT COALESCE(SUM(amount), 0) FROM realized_pnl_events
            WHERE plan_id = ?
              AND substr(created_at, 1, 10) = ?
            """,
            (plan_id, day),
        )
        (pnl,) = await cur.fetchone()
    return {"orders": int(count or 0), "realized_pnl": float(pnl or 0.0)}


# ─── Position ledger ──────────────────────────────────────────────────────────

async def get_position(plan_id: str, symbol: str, strategy: str) -> dict | None:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT * FROM positions WHERE plan_id=? AND symbol=? AND strategy=?",
            (plan_id, symbol, strategy),
        )
        row = await cur.fetchone()
    if row is None:
        return None
    return dict(row)


async def upsert_position(
    plan_id: str, symbol: str, strategy: str, qty: float, avg_price: float
) -> None:
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        await db.execute(
            """
            INSERT INTO positions (plan_id, symbol, strategy, qty, avg_price, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(plan_id, symbol, strategy) DO UPDATE SET
                qty=excluded.qty,
                avg_price=excluded.avg_price,
                updated_at=excluded.updated_at
            """,
            (plan_id, symbol, strategy, qty, avg_price, ts),
        )
        await db.commit()


async def list_positions(plan_id: str | None = None) -> list[dict]:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        if plan_id is not None:
            cur = await db.execute(
                "SELECT * FROM positions WHERE plan_id=? ORDER BY symbol",
                (plan_id,),
            )
        else:
            cur = await db.execute("SELECT * FROM positions ORDER BY plan_id, symbol")
        rows = await cur.fetchall()
    return [dict(r) for r in rows]


# ─── Order intent log ─────────────────────────────────────────────────────────

async def record_intent(
    plan_id: str,
    symbol: str,
    strategy: str,
    side: str,
    qty: float,
    price: float,
    signal: str,
    reason: str,
    status: str,
    realized_pnl: float | None = None,
    reached_broker: bool = False,
) -> int:
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        cur = await db.execute(
            """
            INSERT INTO order_intents
              (created_at, plan_id, symbol, strategy, side, qty, price, signal,
               reason, status, realized_pnl, reached_broker)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (ts, plan_id, symbol, strategy, side, qty, price, signal, reason,
             status, realized_pnl, 1 if reached_broker else 0),
        )
        await db.commit()
        return cur.lastrowid


async def record_realized_pnl(
    plan_id: str,
    symbol: str,
    strategy: str,
    qty: float,
    amount: float,
    source: str,
    order_id: int | None = None,
) -> None:
    """Log a PnL-realizing event on the day it happened.

    `source` is 'submit' for the portion that filled when the order was sent and
    'topup' for quantity the venue filled later. Both must be visible to
    `max_daily_loss`, and on their own days.
    """
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        await db.execute(
            """
            INSERT INTO realized_pnl_events
              (created_at, plan_id, symbol, strategy, qty, amount, source, order_id)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (ts, plan_id, symbol, strategy, qty, amount, source, order_id),
        )
        await db.commit()


async def list_intents(plan_id: str | None = None, limit: int = 50) -> list[dict]:
    await init_db()
    async with aiosqlite.connect(_db_path()) as db:
        db.row_factory = aiosqlite.Row
        if plan_id is not None:
            cur = await db.execute(
                "SELECT * FROM order_intents WHERE plan_id=? ORDER BY id DESC LIMIT ?",
                (plan_id, limit),
            )
        else:
            cur = await db.execute(
                "SELECT * FROM order_intents ORDER BY id DESC LIMIT ?",
                (limit,),
            )
        rows = await cur.fetchall()
    return [dict(r) for r in rows]
