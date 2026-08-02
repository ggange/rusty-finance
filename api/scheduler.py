"""Wall-clock trading loop: refresh bars, then tick every enabled plan.

This is the piece that moves the system from file-replay to a live loop. An
AsyncIOScheduler started in the FastAPI lifespan fires `run_all_plans` on a
weekday cron (default 16:30 America/New_York — after the US close, so the day's
final bar is settled), which refreshes each plan's dataset from Yahoo and then
runs the existing dry-run tick over it.

Safety notes:

* The broker is still `DryRunBroker`. Nothing here can place a real order.
* Holidays need no special handling: the cron fires, the refresh returns no new
  bars, the strategy sees the same last bar it saw yesterday, and `decide()` is
  idempotent — so a holiday tick is a no-op rather than a duplicate order.
* A failure on one plan (or one symbol's fetch) is contained and recorded; it
  does not abort the remaining plans.
"""

import asyncio
import logging
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

from apscheduler.schedulers.asyncio import AsyncIOScheduler
from apscheduler.triggers.cron import CronTrigger

import api.db as db
from api.broker import DryRunBroker
from api.datasets import _data_dir

logger = logging.getLogger(__name__)

RUN_KIND = "scheduled_tick"
JOB_ID = "daily_trade_tick"

_scheduler: AsyncIOScheduler | None = None


# ─── Configuration ────────────────────────────────────────────────────────────

def _env_flag(name: str, default: bool) -> bool:
    raw = os.getenv(name)
    if raw is None:
        return default
    return raw.strip().lower() in {"1", "true", "yes", "on"}


def scheduler_enabled() -> bool:
    """Whether to start the in-process scheduler. Set RUSTY_FINANCE_SCHEDULER=0 to disable."""
    return _env_flag("RUSTY_FINANCE_SCHEDULER", True)


def cron_config() -> dict:
    """Cron fields for the daily tick, overridable by environment."""
    return {
        "day_of_week": os.getenv("RUSTY_FINANCE_SCHEDULER_DAYS", "mon-fri"),
        "hour": int(os.getenv("RUSTY_FINANCE_SCHEDULER_HOUR", "16")),
        "minute": int(os.getenv("RUSTY_FINANCE_SCHEDULER_MINUTE", "30")),
        "timezone": os.getenv("RUSTY_FINANCE_SCHEDULER_TZ", "America/New_York"),
    }


def _refresh_enabled() -> bool:
    """Whether the scheduled job should pull fresh bars before ticking."""
    return _env_flag("RUSTY_FINANCE_SCHEDULER_REFRESH", True)


# ─── Data refresh ─────────────────────────────────────────────────────────────

def _fetcher():
    """Import scripts/fetch_data.py lazily — it lives outside the api package."""
    scripts_dir = str(Path(__file__).resolve().parent.parent / "scripts")
    if scripts_dir not in sys.path:
        sys.path.insert(0, scripts_dir)
    import fetch_data

    return fetch_data


def plan_symbols(plans: list[dict]) -> list[str]:
    """Distinct dataset symbols referenced by a set of plans, in stable order."""
    seen: list[str] = []
    for plan in plans:
        for item in plan.get("items", []):
            symbol = str(item.get("dataset", "")).removesuffix(".csv").upper()
            if symbol and symbol not in seen:
                seen.append(symbol)
    return seen


async def refresh_symbols(symbols: list[str]) -> list[dict]:
    """Incrementally refresh each symbol's CSV. Network I/O runs off the event loop."""
    if not symbols:
        return []
    fetch_data = _fetcher()
    out_dir = _data_dir()

    def _run() -> list[dict]:
        return [fetch_data.fetch_incremental(s, out_dir) for s in symbols]

    try:
        return await asyncio.to_thread(_run)
    except Exception as e:  # a broken fetcher must not stop the tick
        logger.exception("data refresh failed")
        return [{"ticker": s, "status": "error", "error": str(e)} for s in symbols]


# ─── The loop ─────────────────────────────────────────────────────────────────

async def run_all_plans(refresh: bool | None = None) -> dict:
    """Run one full cycle: refresh bars for every enabled plan, then tick each plan.

    Returns a summary dict which is also persisted to the `runs` table under
    kind="scheduled_tick" so /trade/schedule can report the last run.
    """
    from api import trading

    started = datetime.now(timezone.utc).isoformat()
    do_refresh = _refresh_enabled() if refresh is None else refresh

    plans = await db.list_plans(enabled_only=True)
    symbols = plan_symbols(plans)

    refreshed: list[dict] = []
    if do_refresh and symbols:
        refreshed = await refresh_symbols(symbols)

    broker = DryRunBroker()
    plan_results: list[dict] = []

    for plan in plans:
        plan_id = plan["plan_id"]
        try:
            resolved = trading.resolve_items(plan["items"])
            tick = await trading.run_tick(plan_id, resolved, broker)
            plan_results.append({"plan_id": plan_id, "status": "ok", **tick})
        except Exception as e:
            logger.exception("plan %s failed", plan_id)
            plan_results.append({"plan_id": plan_id, "status": "error", "error": str(e)})

    summary = {
        "started_at": started,
        "finished_at": datetime.now(timezone.utc).isoformat(),
        "refreshed": refreshed,
        "plans_run": len(plan_results),
        "plans_failed": sum(1 for p in plan_results if p["status"] == "error"),
        "intents_emitted": sum(
            1
            for p in plan_results
            for r in p.get("results", [])
            if r.get("intent") is not None
        ),
        "results": plan_results,
    }

    await db.save_run(
        RUN_KIND,
        {"refresh": do_refresh, "symbols": symbols, "plan_ids": [p["plan_id"] for p in plans]},
        summary,
    )
    logger.info(
        "scheduled tick: %d plan(s), %d intent(s), %d failure(s)",
        summary["plans_run"], summary["intents_emitted"], summary["plans_failed"],
    )
    return summary


async def _job() -> None:
    """APScheduler entrypoint — never let an exception kill the job."""
    try:
        await run_all_plans()
    except Exception:
        logger.exception("scheduled tick failed")


# ─── Lifecycle ────────────────────────────────────────────────────────────────

def start_scheduler() -> AsyncIOScheduler | None:
    """Start the in-process scheduler. Returns None when disabled by env."""
    global _scheduler
    if not scheduler_enabled():
        logger.info("scheduler disabled (RUSTY_FINANCE_SCHEDULER=0)")
        return None
    if _scheduler is not None:
        return _scheduler

    cfg = cron_config()
    sched = AsyncIOScheduler(timezone=cfg["timezone"])
    sched.add_job(
        _job,
        CronTrigger(
            day_of_week=cfg["day_of_week"],
            hour=cfg["hour"],
            minute=cfg["minute"],
            timezone=cfg["timezone"],
        ),
        id=JOB_ID,
        replace_existing=True,
        max_instances=1,
        coalesce=True,  # a missed window fires once, not once per miss
    )
    sched.start()
    _scheduler = sched
    logger.info(
        "scheduler started: %s %02d:%02d %s",
        cfg["day_of_week"], cfg["hour"], cfg["minute"], cfg["timezone"],
    )
    return sched


def shutdown_scheduler() -> None:
    global _scheduler
    if _scheduler is not None:
        _scheduler.shutdown(wait=False)
        _scheduler = None


async def schedule_status() -> dict:
    """Current scheduler state for GET /trade/schedule."""
    cfg = cron_config()
    next_run = None
    if _scheduler is not None:
        job = _scheduler.get_job(JOB_ID)
        if job is not None and job.next_run_time is not None:
            next_run = job.next_run_time.isoformat()

    return {
        "enabled": scheduler_enabled(),
        "running": _scheduler is not None,
        "refresh_data": _refresh_enabled(),
        "cron": cfg,
        "next_run": next_run,
        "last_run": await db.last_run_of_kind(RUN_KIND),
    }
