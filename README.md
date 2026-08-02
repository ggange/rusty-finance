# rusty-finance

A financial backtesting monorepo: a high-performance Rust core, Python bindings via PyO3, a FastAPI REST layer, and a Vite+React 18 frontend.

```
backtesting/          Rust library — strategies, engine, portfolio, metrics
backtesting-py/       PyO3 bridge — exposes run() and run_portfolio() to Python
api/                  FastAPI server — REST endpoints over the Python bindings
frontend/             Vite + React 18 UI — portfolio form, results dashboard
data/datasets/        CSV files for data-source picker (configurable directory)
data/fixtures/        Synthetic CSV fixtures used by tests
```

## Architecture

```
backtesting (Rust core)
      ↓  path dependency
backtesting-py (PyO3 cdylib)
      ↓  import backtesting_py
api (FastAPI HTTP layer)
```

## Prerequisites

| Tool | Version |
|------|---------|
| Rust | stable (edition 2024) |
| Python | ≥ 3.9 |
| Node.js | ≥ 16 |
| maturin | ≥ 1.0 |

Install Python dependencies: `pip install maturin`

Node dependencies installed via `npm install` in the `frontend/` directory.

## Quick Start

The project uses a Python virtualenv at `.venv/`. Activate it first in any terminal that runs Python commands:

```bash
source .venv/bin/activate   # macOS/Linux
# .venv\Scripts\activate    # Windows
```

If `.venv` doesn't exist yet, create it and install deps:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install maturin
pip install -e "api/[dev]"
```

Run the full app with three terminals (all from the repo root, with `.venv` active):

**Terminal 1 — Build Python bindings:**
```bash
source .venv/bin/activate
cd backtesting-py
maturin develop
```
(Re-run after Rust changes; bindings are installed into `.venv`.)

**Terminal 2 — Start FastAPI backend:**
```bash
source .venv/bin/activate
uvicorn api.main:app --reload
```
Backend runs at `http://localhost:8000`. Swagger docs at `/docs`.

**Terminal 3 — Start Vite dev server:**
```bash
cd frontend
npm install    # first time only
npm run dev
```
Frontend runs at `http://localhost:5173` and proxies API calls to `localhost:8000` (via `vite.config.ts`).

Open `http://localhost:5173` in your browser.

## Build & Test

### Rust core

```bash
cd backtesting
cargo test          # unit + integration tests
cargo doc --open    # browse rustdoc
```

### Python bindings

```bash
cd backtesting-py
maturin develop     # build and install into current virtualenv
```

### API

Run from the repo root:

```bash
pip install -e "api/[dev]"
uvicorn api.main:app --reload   # start server at http://localhost:8000
pytest api/tests/               # run API tests
```

## CSV Format

The backtesting engine expects CSV files with these headers:

| Column | Type | Description |
|--------|------|-------------|
| Date | YYYY-MM-DD | Bar date |
| Open | float | Opening price |
| High | float | Period high |
| Low | float | Period low |
| Close | float | Closing price |
| Volume | integer | Shares traded |

## Market Data

Datasets live in `data/datasets/` as one CSV per symbol. Two ways to populate them:

```bash
make fetch TICKER=NVDA          # full download over a date range
make fetch-all                  # full download of the standard 5-symbol catalog
make refresh                    # append only bars newer than what's on disk
make refresh SYMBOLS="AAPL TSLA"
```

`make refresh` is the incremental path: for each symbol it reads the last stored
date, refetches from that bar forward, and merges. Rows are deduplicated on
`Date` with the newly fetched bar winning, so the trailing bar picks up Yahoo's
post-close revisions rather than keeping a stale copy. On a network error or an
empty response the existing CSV is left untouched — a failed refresh never
destroys history.

## Trading Loop (dry-run)

The loop turns a strategy signal on the latest bar into an order intent. It is
**dry-run only** — `DryRunBroker` logs intents and writes them to SQLite; no
broker is connected and no real order can be placed.

Store a plan, and the scheduler will run it:

```bash
curl -X POST localhost:8000/trade/plans -H 'Content-Type: application/json' -d '{
  "plan_id": "paper-rsi",
  "items": [
    {"dataset": "MSFT.csv", "strategy": {"type": "rsi", "period": 14}, "cash_allocation": 10000},
    {"dataset": "NVDA.csv", "strategy": {"type": "rsi", "period": 14}, "cash_allocation": 10000}
  ]
}'

curl localhost:8000/trade/schedule            # cron config, next run, last run
curl -X POST localhost:8000/trade/schedule/run   # run the cycle now
curl localhost:8000/trade/positions
curl localhost:8000/trade/intents
```

An in-process APScheduler starts with the API and fires the cycle on a weekday
cron — refresh every plan symbol's bars, then tick each enabled plan. Defaults
to 16:30 America/New_York, after the US close so the day's final bar is settled.

| Env var | Default | Purpose |
|---------|---------|---------|
| `RUSTY_FINANCE_SCHEDULER` | `1` | Set `0` to disable the scheduler entirely |
| `RUSTY_FINANCE_SCHEDULER_DAYS` | `mon-fri` | Cron day-of-week field |
| `RUSTY_FINANCE_SCHEDULER_HOUR` | `16` | Cron hour |
| `RUSTY_FINANCE_SCHEDULER_MINUTE` | `30` | Cron minute |
| `RUSTY_FINANCE_SCHEDULER_TZ` | `America/New_York` | Scheduler timezone |
| `RUSTY_FINANCE_SCHEDULER_REFRESH` | `1` | Set `0` to tick without fetching data |

Market holidays need no special handling: the cron fires, the refresh returns no
new bars, the strategy re-reads the same last bar, and the buy/sell decision is
idempotent — so a holiday tick is a no-op rather than a duplicate order. The same
property makes `POST /trade/schedule/run` safe to invoke repeatedly.

## Troubleshooting

### Backend

**"Address already in use" when starting uvicorn**
```
ERROR:    [Errno 48] Address already in use
```
Another process is listening on port 8000. Either:
- Kill the process: `lsof -ti:8000 | xargs kill -9`
- Or use a different port: `uvicorn api.main:app --reload --port 8001` (then update frontend proxy in `frontend/vite.config.ts`)

**"ModuleNotFoundError: No module named 'backtesting_py'"**
The Python bindings haven't been built. Run `cd backtesting-py && maturin develop` and ensure you're in the same Python venv.

**"Connection refused" when frontend tries to reach backend**
- Check backend is running: `curl http://localhost:8000/health`
- If using a different backend port, update the proxy in `frontend/vite.config.ts` (line `target: "http://localhost:8000"`)

### Frontend

**"Module not found" or build fails**
```bash
cd frontend
npm install
npm run build
```
Rebuild TypeScript and dependencies.

**Vite dev server port 5173 already in use**
```bash
npm run dev -- --port 5174
```
Use a different port and update your browser URL.

**"Engine unavailable" message in the UI**
The FastAPI backend isn't responding or the bindings aren't built. Check:
1. `.venv` is activated (`source .venv/bin/activate`)
2. `maturin develop` completed successfully (from `backtesting-py/`)
3. `uvicorn api.main:app --reload` is running (from repo root)
4. `curl http://localhost:8000/health` returns `{"status":"ok","engine":"available"}`

### Data

**Dataset dropdown shows "Pick a dataset…" but no options**
- Place CSV files in `data/datasets/` (default, or set `RUSTY_FINANCE_DATA_DIR=<path>`)
- Restart the backend for changes to take effect
- Each CSV must have a `Date, Open, High, Low, Close, Volume` header

**"Unknown dataset" error when running a backtest**
The dataset name doesn't exist in the catalog. Verify the file is in `data/datasets/` and the backend can see it via `GET /datasets`.
