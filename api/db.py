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
"""


def _db_path() -> Path:
    return Path(os.getenv("RUSTY_FINANCE_DB", "data/runs.db"))


async def init_db() -> None:
    path = _db_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    async with aiosqlite.connect(path) as conn:
        await conn.execute(_CREATE_SQL)
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
