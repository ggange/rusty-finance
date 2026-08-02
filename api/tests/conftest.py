"""Shared test configuration.

The app starts an in-process scheduler in its lifespan, and every TestClient
runs that lifespan. Tests must not spin up a real cron thread, so the scheduler
is off by default here; the scheduler tests opt back in explicitly.
"""

import os

os.environ.setdefault("RUSTY_FINANCE_SCHEDULER", "0")
