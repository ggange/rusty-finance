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
    status     TEXT NOT NULL
);
"""


def _db_path() -> Path:
    return Path(os.getenv("RUSTY_FINANCE_DB", "data/runs.db"))


async def init_db() -> None:
    path = _db_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    async with aiosqlite.connect(path) as conn:
        await conn.executescript(_CREATE_SQL)
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
) -> int:
    await init_db()
    ts = datetime.now(timezone.utc).isoformat()
    async with aiosqlite.connect(_db_path()) as db:
        cur = await db.execute(
            """
            INSERT INTO order_intents
              (created_at, plan_id, symbol, strategy, side, qty, price, signal, reason, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (ts, plan_id, symbol, strategy, side, qty, price, signal, reason, status),
        )
        await db.commit()
        return cur.lastrowid


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
