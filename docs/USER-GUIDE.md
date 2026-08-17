# rusty-finance — User Guide

How to actually use the platform you built. The README is a reference; this is a
walkthrough. It goes in the order you'd naturally work: get it running, test an
idea on one asset, combine assets into a portfolio, check whether the result is
real or luck, then let it trade on paper.

Every command here is copy-pasteable from the repo root.

**Contents**

1. [Getting it running](#1-getting-it-running)
2. [The mental model](#2-the-mental-model)
3. [Data](#3-data)
4. [Portfolio backtest tab](#4-portfolio-backtest-tab)
5. [Reading the results](#5-reading-the-results)
6. [Portfolio weights](#6-portfolio-weights)
7. [Parameter sweep tab](#7-parameter-sweep-tab)
8. [Walk-forward tab](#8-walk-forward-tab)
9. [Trading tab](#9-trading-tab)
10. [A worked session](#10-a-worked-session-start-to-finish)
11. [Using the API directly](#11-using-the-api-directly)
12. [Common mistakes](#12-common-mistakes-this-platform-lets-you-make)

---

## 1. Getting it running

First time only:

```bash
make setup      # creates .venv, installs API + dev deps, npm install
```

Every session:

```bash
make dev        # rebuilds Rust bindings, starts API :8000, then Vite :5173
```

Open http://localhost:5173. The header shows an **engine** pill — it must read
`engine: available`. If it says `unavailable`, the Rust extension module didn't
build; run `make bindings` and read the error.

Interactive API docs are at http://localhost:8000/docs (FastAPI generates them
from the same Pydantic models the UI posts to, so they're never stale).

Other entry points:

| Command | What it does |
|---|---|
| `make test` | Rust tests, Python tests, frontend type-check + build |
| `make bindings` | Rebuild Rust→Python bindings. **Run after any `.rs` edit** |
| `make fetch TICKER=TSLA` | Download one ticker's OHLCV into `data/datasets/` |
| `make fetch-all` | AAPL MSFT GOOG SPY NVDA, 2020–2024 |
| `make refresh` | Append only new bars to existing datasets |
| `cd frontend && npm test` | Vitest in watch mode |

> **The one gotcha that bites everyone:** editing Rust and reloading the browser
> changes nothing. The API imports a *compiled* module. `make bindings` (or
> `make dev`, which does it for you) is what makes Rust edits take effect.

---

## 2. The mental model

Four layers, each one a thin wrapper on the one below:

```
React UI  (:5173)      what you click
   ↓ fetch /api/*
FastAPI   (:8000)      validation, dataset loading, persistence (SQLite)
   ↓ PyO3
backtesting_py         Python↔Rust bridge — JSON in, JSON out
   ↓
Rust core              the actual simulation, metrics, optimizer
```

The API does **no** finance. It validates your request, reads CSVs off disk,
hands JSON to Rust, and stores the answer. That's why the Rust tests are the
ones that matter, and why every parameter constraint you see in the UI
(`short_window < long_window`) is enforced in Pydantic *and* honoured by Rust.

Three concepts you need before anything else makes sense:

**A strategy is a signal function.** Given bars up to time *t*, it emits buy,
sell, or nothing. It does not decide position size. Six are built in:

| `type` | Name | Signal |
|---|---|---|
| `ma_ema` | EMA crossover | short EMA crosses long EMA |
| `ma_sma` | SMA crossover | short SMA crosses long SMA |
| `ma_wma` | WMA crossover | short WMA crosses long WMA |
| `rsi` | RSI | buy < 30, sell > 70 |
| `macd` | MACD | MACD line crosses signal line |
| `bollinger_bands` | Bollinger | close outside the band |

**Fill timing is where backtests lie.** A signal computed from bar *t*'s close
cannot be filled at bar *t*'s close — you didn't know the close until it
happened. Default is `next_open`. The `close` option exists only for comparison
with naive backtests; **any result you show anyone should use `next_open`.**

**Weights are separate from signals.** Each asset gets a share of capital; its
strategy decides when to be in or out of *its own* sleeve. Rebalancing moves
capital between sleeves. These are independent — see §6.

---

## 3. Data

Datasets are CSVs in `data/datasets/`, referenced by filename (`AAPL.csv`).
Shipped: AAPL, GOOG, MSFT, NVDA, SPY.

```bash
make fetch TICKER=TSLA START=2018-01-01 END=2024-12-31
make refresh SYMBOLS="AAPL MSFT"     # incremental append
```

Format — header required, one row per bar, ascending by date:

```csv
date,open,high,low,close,volume
2020-01-02,74.06,75.15,73.80,75.09,135480400
```

You can also drag a CSV into the UI's data-source picker. Uploaded candles are
sent inline with the request and are **not** saved to the catalog — good for a
one-off, wrong for anything you'll want to reproduce. Put it in
`data/datasets/` if you'll use it twice.

Point at a different directory with `RUSTY_FINANCE_DATA_DIR`.

---

## 4. Portfolio backtest tab

This is the main workspace, and it handles the single-asset case too — one
asset with 100% weight *is* a single-asset backtest.

**Left column — configuration.**

- **Assets.** Each row is: data source (catalog dropdown or upload) → strategy →
  that strategy's parameters → optional weight. Add rows with the button below
  the list. Weights that don't sum to 1 are normalised.
- **Initial cash.** Default 10,000. It's the total across all sleeves.
- **Commission** — flat currency amount per trade. **Slippage %** — fraction of
  price lost to execution, so `0.001` = 10 bps. Both default to zero, which is a
  fantasy; set them before believing any number.
- **Fill timing.** Leave on `Next open`.
- **External benchmark.** A dataset name (`SPY.csv`) overlaid on the equity
  chart as buy-and-hold. This is the comparison that matters — beating cash is
  trivial, beating the index is the question.
- **Rebalance.** Off by default (weights drift with performance). Options:
  `monthly`, `quarterly`, or `threshold` with a drift fraction — the last one
  rebalances only when some asset strays more than *x* from target, which
  trades less than the calendar options.
- **Weight policy.** See §6.

**Right column — results.** Equity curve, metrics, drawdown, rolling
volatility, correlation matrix, per-asset breakdown, trade log. Under it, run
history: every run is written to SQLite (`data/runs.db`) and clicking a past run
reloads its full result.

---

## 5. Reading the results

| Metric | What it means | How to read it |
|---|---|---|
| `total_return` | Final ÷ initial − 1 | Meaningless without the period length |
| `cagr` | Annualised return | Compare against the benchmark's CAGR, never zero |
| `annualized_volatility` | σ of daily returns × √252 | The denominator of everything below |
| `sharpe_ratio` | Excess return ÷ volatility | Below ~1 on a backtest is noise. Above ~3 means you have a bug |
| `sortino_ratio` | Same, but only downside σ | Higher than Sharpe when losses are small and frequent |
| `max_drawdown` | Worst peak-to-trough | The number that decides whether you'd have held on |
| `win_rate` | Fraction of profitable trades | Nearly uninformative alone — a 30% win rate is fine if winners are large |
| `trade_count` | Round trips | Under ~30, your metrics are anecdotes |

**Risk panel.** `var_95` / `cvar_95` (and the 99% pair) are the loss you exceed
5% of days, and the average loss *when* you exceed it. CVaR is the honest one:
VaR tells you where the cliff is, CVaR tells you how far down it goes.
`contribution_to_risk` decomposes portfolio variance by asset — it is routinely
nothing like the capital weights, and that gap is the whole motivation for §6.

**How to be sceptical, in order:**

1. Is `trade_count` big enough to mean anything?
2. Does it still work with realistic commission and slippage?
3. Does it beat the SPY overlay, or just beat cash?
4. Is the equity curve one lucky trade with a flat line either side?
5. Would you have held through `max_drawdown`?
6. Does it survive walk-forward (§8)?

Only step 6 is evidence. Everything before it is a filter.

---

## 6. Portfolio weights

Three policies, chosen in the config panel:

**Manual** (default) — you set weights. Fine when you have a reason.

**Static** — observe `warmup` bars (default 252 ≈ one trading year), solve once
on that window, hold those weights for the rest of the run. The solve sees only
the warm-up, so it cannot peek at the future.

**Dynamic** — re-solve at *every* rebalance date using a trailing `lookback`
window. Solved weights only take effect at a rebalance, so if you don't set a
frequency the engine falls back to monthly rather than silently never
re-solving — but set one deliberately. Nothing is solved until a full `lookback`
of bars exists, so the first stretch of the run uses your manual weights. Each
solve sees only the bars before it. This is the interesting mode
and the one Rust's speed makes free: a solve costs 6–15 µs, so ~78 solves add
about 1 ms to a 9 ms run. Compute is not your constraint; estimation error is.

Five objectives:

| Objective | Solves for | Needs |
|---|---|---|
| `equal_weight` | 1/n | nothing |
| `inverse_volatility` | weight ∝ 1/σ | volatilities |
| `min_variance` | lowest portfolio variance | full covariance |
| `risk_parity` | equal *risk* contribution per asset | full covariance |
| `max_sharpe` | highest expected Sharpe | covariance **and mean returns** |

`max_sharpe` is the one to distrust. Mean returns are estimated far less
reliably than covariance — you need decades of data to pin a mean down, and
markets don't sit still that long. The API flags this back to you as
`uses_expected_returns: true`. In this repo's own benchmark, dynamic max-Sharpe
produced the highest total return (193.02% vs 148.60% for equal weight) and a
*worse* Sharpe ratio (0.77 vs 0.80) with a deeper drawdown (−34.02% vs −27.55%).
It optimised for Sharpe and delivered less of it. That is estimation error, in
one line. Reproduce the full table with
`cargo run --release -p backtesting --example optimize_bench`.

Two knobs on every objective:

- **Shrinkage** (0–1, default 0.2) pulls the covariance matrix toward its
  diagonal. Sample covariance is noisy, and the noise concentrates in exactly
  the small eigenvalues that optimisers love to load up on. Shrinkage is
  deliberately biasing your estimate to reduce its variance, and it is almost
  always worth it. 0 = raw sample, 1 = ignore correlations entirely.
- **Max weight** caps any single position. Uncapped min-variance will happily
  put 90% in one asset.

All solutions are long-only and sum to 1 (projected onto the simplex). There is
no shorting and no leverage.

**Solving weights on its own,** without a backtest, via `POST /portfolio/optimize` —
useful for inspection, but note it fits to the window you hand it and is
therefore not evidence of anything. To find out whether an objective *helps*,
run a `/portfolio` backtest with a `weight_policy` and read the realized curve.

---

## 7. Parameter sweep tab

Runs one strategy over a grid of parameters on one dataset, and returns
`{params, metrics}` per combination. Pick a dataset, a strategy type, then set
`min`/`max`/`step` per parameter. Combinations violating a constraint (e.g.
`short_window >= long_window`) are skipped silently, so a grid of 400 may yield
190 results.

**What it's for:** seeing the *shape* of the parameter surface. A strategy whose
Sharpe is 1.4 at period 14 and 0.2 at 13 and 15 is not a strategy, it's a
coincidence — you want a broad plateau, not a spike.

**What it is not for:** picking parameters. Choosing the grid's best point is
the purest form of overfitting available to you; the more combinations you try,
the higher the best in-sample number goes, whether or not there's any signal.
That's what the next tab is for.

**The panel now quantifies that for you.** Under the green "best combination"
callout sits the **Deflated Sharpe Ratio** (Bailey & López de Prado 2014): the
probability that the winning cell's Sharpe beats what a search of this size
reaches with no skill at all. It is shown beside the *uncorrected* figure, and the
gap between them is the cost of the search. On MSFT with RSI 5→30 — 26 cells —
the best cell scores Sharpe 0.55, which is a 92 % chance of beating zero but only
a **71 %** chance of beating the best of 26 coin flips. Below 95 %, the badge says
so.

Two things to know about the number:

- **It counts the trials in *this* grid only.** It cannot see the other
  strategies, the other assets, or the grids you ran yesterday, so treat it as an
  *upper bound* on significance. The "trials to deflate against" field lets you
  supply the honest count; a value below the grid size is rejected rather than
  honoured.
- **It describes the Sharpe-selected cell**, whatever metric the chart is ranking
  by, because the formula's distributional assumptions are Sharpe's. When you
  switch the metric the panel says this explicitly.

---

## 8. Walk-forward tab

The honest test, and the reason to trust anything here.

The dataset is split into `n_windows` rolling folds. In each fold, the first
`train_frac` (default 0.7) is used to pick the best parameters by `metric`
(default `sharpe_ratio`); those parameters are then evaluated on the held-out
remainder. You get one train result and one test result per fold.

**Read the gap, not the level.** Train Sharpe is always good — it was selected
to be. The questions are: does test Sharpe stay positive across *most* folds,
and do the selected parameters stay in the same neighbourhood? Parameters that
jump from 5 to 40 to 12 across folds are fitting noise, and the strategy has no
stable operating point regardless of how the test numbers look.

**Don't read the magnitude at all — and the interval under each fold now shows
you why.** A Sharpe from a ~75-bar test fold has a standard error near ±1.8 once
annualised, so a fold reporting 1.5 and a fold reporting 0.2 are not meaningfully
different, and a test Sharpe *above* train tells you the folds are too short to
compare rather than that the strategy is robust. Sign consistency and parameter
stability are the signals here. The level is noise wearing a decimal point.

You no longer have to take that on trust: every fold's test cell carries a
bootstrap confidence interval, and on real data they come out roughly
`[−3, +3]` — visibly uninformative, which is the honest reading.

**Read the "Pooled out-of-sample" panel first.** It stitches every fold's
out-of-sample returns into one series and reports the interval for *that*. It is
deliberately not the average of the folds: averaging treats each fold as an
independent observation, while pooling treats the sequence as the single return
path it actually is. Five folds of ~100 bars average to a number with no error
bar; the same returns pooled are ~500 observations and support a real statement.
If the pooled interval contains zero, the panel says so — that badge is the most
important thing on the screen.

`docs/strategy-validation.md` has this already run across all six strategies —
RSI and Bollinger survived, MACD and EMA crossover were eliminated. Read it
before spending a week rediscovering it, and read its caveat block before quoting
any number from it.

**Current gaps worth knowing:**

- **Intervals cover sampling error, not selection.** Sharpe, Sortino and CAGR now
  report a bootstrap confidence interval on the Backtest and Walk-forward tabs;
  max drawdown reports a spread only, because block resampling breaks up the
  multi-month trends that produce deep drawdowns and percentile endpoints there
  would read as optimistic. What no interval fixes: a band around a *selected*
  maximum has no valid frequentist reading, so the sweep's "best combo" callout
  needed a different correction and now carries one — see the Deflated Sharpe
  Ratio in §7. The sweep grid still deliberately shows no intervals at all: a grid
  of independent 95 % bands invites exactly that misreading. Still uncorrected:
  trial multiplicity *across* sweeps, assets and folds, which needs the
  probability of backtest overfitting.
- **A per-fold interval is not an interval for an average across folds**, and
  folds over correlated assets in the same calendar window are not independent
  observations. Use the pooled panel for a single-asset run; there is no
  cross-asset pooling yet.
- **Walk-forward validates strategy *parameters*, not weighting *objectives*.**
  You can ask "which RSI period holds up out-of-sample"; you can't yet ask "which
  weighting objective does." Given the max-Sharpe result in §6, that remains a
  real gap.

---

## 9. Trading tab

The live loop, still on paper. Monitoring-first layout: a status strip that
answers "is this thing OK?", a control column, and live-state tables.

### The pieces

**Broker** — set by environment variable, one of two:

| `RUSTY_FINANCE_BROKER` | Behaviour |
|---|---|
| `dry_run` (default) | Positions in process memory. Restarting loses the venue's books, so reconcile will report drift — expected, not a bug |
| `paper_sim` | Persistent simulated venue; books survive restart. **Use this for a soak** |

```bash
RUSTY_FINANCE_BROKER=paper_sim \
RUSTY_FINANCE_BROKER_SLIPPAGE=0.0008 \
RUSTY_FINANCE_BROKER_FILL_RATIO=1.0 \
  .venv/bin/uvicorn api.main:app --reload
```

**A plan** is what gets traded: a `plan_id` plus items of
`dataset + strategy + cash_allocation`, and an `enabled` flag the scheduler
respects. Datasets are validated when you save, because the scheduler runs
unattended and shouldn't discover a bad path at 16:30.

**A tick** is one pass: for each item, load data, ask the strategy for its
latest signal, check the ledger for existing exposure, and emit an intent.
Ticks are idempotent — a second tick while already long does not buy again.

**The scheduler** fires the tick automatically. Defaults: `mon-fri` at `16:30`
`America/New_York`, refreshing bars first. Override with
`RUSTY_FINANCE_SCHEDULER_{DAYS,HOUR,MINUTE,TZ,REFRESH}`, disable with
`RUSTY_FINANCE_SCHEDULER=0`.

**Risk limits** are two layers: a global row and an optional per-plan row
layered over it *field by field* — a plan setting only `max_position_value`
still inherits the global `max_daily_loss`. The UI shows `effective`, `plan` and
`global` side by side so the merge is visible rather than guessed. Three limits:
`max_position_value`, `max_daily_loss`, `max_daily_orders` (the runaway-loop
guard). Unset means unlimited, and an unconfigured install logs a warning on
every boot for exactly that reason.

**The kill switch** halts all order submission — including exits. It lives in
SQLite, so a halted system stays halted across restarts until a human releases
it. Engaging is one click; **disengaging requires typing the confirmation
phrase**, because that's the direction that re-arms capital.

### The tables

- **Positions** — ledger holdings. Quantities are fractional; positions are
  sized by cash.
- **Orders** — status, filled vs requested qty, average fill price, rejection
  reason.
- **Intents** — what the strategy *wanted*, including intents the guardrails
  rejected. This is where you see risk limits doing their job.
- **Soak** — fill rate, order counts, and realized slippage in bps. **This is
  the platform grading itself.** The backtester assumes a fill at the signal
  price; the gap to the actual fill is the modelling error. Near-zero mean
  slippage with a high fill rate means the backtest is honest. A persistent
  adverse skew means your backtest is optimistic by that much, and every result
  in §5 should be discounted accordingly. Note the `note` field: with
  `paper_sim`, slippage is *configured*, not observed — it only becomes evidence
  against a real venue.
- **Reconcile** — broker holdings vs our ledger, per symbol, with deltas. Empty
  drift is the good state. Drift means your record of reality is wrong, so every
  decision made from it is suspect. Runs automatically on every tick.

### Running a soak

1. Start with `paper_sim` and non-zero slippage.
2. Create a plan from strategies that survived walk-forward (RSI, Bollinger).
3. Set global limits *before* the first tick.
4. Fire a manual tick and confirm orders, positions and intents populate and
   reconcile reports in sync.
5. Let the scheduler run for weeks.
6. Watch the Soak panel. You are looking for the realized slippage to match what
   you assumed in §4, and for the fill rate to stay high.

---

## 10. A worked session, start to finish

The path from "I wonder if RSI works" to "it's trading on paper":

```bash
make dev
```

1. **Sweep** (Parameter sweep tab): RSI on AAPL.csv, `period` 5→30 step 1.
   Look at the surface. Is there a plateau, or a spike? Then read the deflated
   Sharpe under the best-combination box: if the winner doesn't survive the size
   of its own search, the shape of the surface is the only thing here worth
   carrying forward.
2. **Walk-forward** (Walk-forward tab): same grid, 5 windows, train 0.7,
   metric `sharpe_ratio`. Are test Sharpes positive in most folds? Do the
   selected periods cluster? Ignore how *large* the test Sharpes are — at this
   fold length the magnitudes carry no information.
3. **If it survives**, back to the Portfolio backtest tab: AAPL + MSFT + NVDA,
   RSI at the period the folds agreed on, commission 1.0, slippage 0.001,
   fill timing `next_open`, benchmark `SPY.csv`.
4. **Add a weight policy**: dynamic, risk parity, quarterly rebalance,
   252-bar lookback. Compare against manual equal weights. Did risk parity
   improve the Sharpe, or just move return around?
5. **Check the drawdown.** Would you have held through it? If not, stop here —
   a strategy you'd abandon at the worst moment has an expected return of
   whatever you'd have realised at that moment.
6. **Trading tab**: create the plan, set limits, tick once by hand, verify.
7. **Let it soak** for a month and watch the slippage number.

If step 2 fails, delete the idea. That's the point of step 2.

---

## 11. Using the API directly

Everything the UI does is a plain HTTP call; http://localhost:8000/docs is
browsable and lets you fire requests from the page.

Solve weights on their own:

```bash
curl -s localhost:8000/portfolio/optimize -H 'content-type: application/json' -d '{
  "datasets": ["AAPL.csv", "MSFT.csv", "NVDA.csv", "SPY.csv"],
  "lookback": 252,
  "optimizer": {"objective": "risk_parity", "shrinkage": 0.2}
}' | python3 -m json.tool
```

A dynamic-weight portfolio backtest:

```bash
curl -s localhost:8000/portfolio -H 'content-type: application/json' -d '{
  "assets": [
    {"symbol": "AAPL", "source": {"kind": "dataset", "name": "AAPL.csv"},
     "strategy": {"type": "rsi", "period": 14}},
    {"symbol": "MSFT", "source": {"kind": "dataset", "name": "MSFT.csv"},
     "strategy": {"type": "rsi", "period": 14}}
  ],
  "initial_cash": 100000,
  "commission": 1.0,
  "slippage_pct": 0.001,
  "benchmark_symbol": "SPY.csv",
  "rebalance": {"frequency": {"kind": "quarterly"}},
  "weight_policy": {
    "kind": "dynamic",
    "lookback": 252,
    "optimizer": {"objective": "min_variance", "shrinkage": 0.2, "max_weight": 0.5}
  }
}' | python3 -m json.tool
```

Trading loop:

```bash
curl -s localhost:8000/trade/broker
curl -s -X POST localhost:8000/trade/limits -H 'content-type: application/json' \
  -d '{"max_position_value": 25000, "max_daily_loss": 1000, "max_daily_orders": 20}'
curl -s -X POST localhost:8000/trade/plans -H 'content-type: application/json' -d '{
  "plan_id": "default",
  "items": [{"dataset": "MSFT.csv", "strategy": {"type":"rsi","period":14},
             "cash_allocation": 10000}]
}'
curl -s -X POST localhost:8000/trade/tick -H 'content-type: application/json' \
  -d '{"plan_id": "default"}'
curl -s "localhost:8000/trade/soak?plan_id=default"
curl -s -X POST localhost:8000/trade/killswitch -H 'content-type: application/json' \
  -d '{"engaged": true, "reason": "manual halt"}'
```

Full route list: `/health`, `/strategies`, `/datasets`, `/backtest`,
`/portfolio`, `/portfolio/optimize`, `/sweep`, `/walkforward`, `/runs`, and the
`/trade/*` subsystem (plans, limits, killswitch, schedule, tick, positions,
orders, intents, broker, soak, reconcile).

---

## 12. Common mistakes this platform lets you make

Each of these produces a *plausible* number, which is what makes them dangerous.

1. **Zero costs.** Defaults are 0 commission, 0 slippage. A strategy trading 200
   times a year at 10 bps loses 2% annually that the default run never shows.
2. **`fill_timing: "close"`.** Buys at a price you couldn't have known. It
   inflates every high-frequency signal.
3. **Picking the sweep's best cell.** In-sample maximum, by construction. Use
   walk-forward — and read the deflated Sharpe first, which tells you how much of
   that maximum the search bought. A cell below 95 % there is not a candidate.
4. **Judging by total return.** Higher return with a worse Sharpe and a deeper
   drawdown is a worse strategy. §6's max-Sharpe result is exactly this trap.
5. **Trusting `max_sharpe`.** It needs expected returns, which are the hardest
   quantity in finance to estimate.
6. **Reading a solved-weights panel as a recommendation.** It's fitted to the
   window you chose. Only the realized curve from a weight *policy* is evidence.
7. **No benchmark.** Beating cash in a bull market is not skill. Set
   `benchmark_symbol`.
8. **Too few trades.** Twelve trades cannot support a Sharpe ratio.
9. **Soaking on `dry_run`.** Its books are in process memory; reconcile drift is
   guaranteed and meaningless. Use `paper_sim`.
10. **Running live with no limits.** Unset means unlimited. The API warns on
    every boot; the warning is not decorative.

---

## Where to go next

- `docs/READING-LIST.md` — the theory behind all of the above
- `docs/ROADMAP.md` — what's built, what's next, and the decision log explaining
  *why* things are the way they are
- `docs/strategy-validation.md` — walk-forward results for all six strategies
- `README.md` — installation reference and troubleshooting
