# rusty-finance — Roadmap

A living document for the long run. The short-run, actively-worked plan lives in
issues / the current working session; this file is the map, not the turn-by-turn.

Last updated: 2026-06-23

---

## Where we are today

A fully real-data, multi-strategy backtesting and research stack, end to end:

- **Rust core** (`backtesting/`) — event-driven `BacktestEngine`, a `Portfolio`
  with cash/share accounting, execution costs and sizing rules, a `Strategy`
  trait, and an 8-metric analytics layer (total return, CAGR, vol, max drawdown,
  Sharpe, Sortino, win rate, trade count). Multi-asset orchestration
  (`portfolio_backtest.rs`) aggregates per-asset runs into one portfolio result,
  with optional NAV-overlay rebalancing (monthly, quarterly, drift-threshold).
- **Strategies** — four families: moving-average crossover (SMA/EMA/WMA), RSI,
  MACD (EMA-seeded crossover), and Bollinger Bands (population std-dev bands).
- **Risk analytics** (`risk.rs`) — Pearson correlation matrix, annualised covariance,
  per-asset beta and contribution-to-risk, 21-bar rolling volatility,
  historical VaR/CVaR at 95 % and 99 %.
- **Bindings** (`backtesting-py/`) — PyO3 surface: `run`, `run_portfolio`, `run_sweep`, CSV helpers.
- **API** (`api/`) — FastAPI: `/health`, `/strategies`, `/backtest`, `/datasets`,
  `/datasets/{name}`, `/portfolio`, `/sweep`, `/runs`, `/runs/{id}`.
- **Frontend** (`frontend/`) — React 18 + Vite 8, multi-asset portfolio form,
  results dashboard (equity/drawdown/price charts, risk analytics panel,
  per-asset drill-down), run history panel (click to restore any past result),
  parameter sweep tab (bar chart / 2D heatmap).
- **Data** — real OHLCV for AAPL, MSFT, GOOG, SPY, NVDA (2020-2024, yfinance,
  split/dividend-adjusted). Synthetic fixtures for tests. `make fetch` for new tickers.
- **Persistence** — every backtest and portfolio run saved to SQLite (`data/runs.db`).
- **Dev tooling** — `make dev` / `make test` / `make bindings`, one-command startup
  with health gate, GitHub Actions CI (cargo test + pytest + npm build).

What this is **not** yet: a *trading* system. It backtests on historical CSVs and
fills at close — there is no live data feed, no broker connectivity, no order
management, and no out-of-sample validation of strategies. It is a research tool,
and the next horizons turn it into something you can trade with — paper first.

---

## Guiding principles

1. **The Rust core stays pure-compute and well-tested.** Data resolution, I/O,
   and orchestration of side effects live in Python/API; the engine takes candles
   and returns results. TDD red→green stays the convention.
2. **Each layer earns its keep.** Don't add an API endpoint without a core
   capability behind it, or a UI control without an endpoint.
3. **Realism before breadth.** A few strategies on *real* data beats twenty
   strategies on synthetic noise.
4. **Educational value is a feature.** This began as a Rust learning project;
   prefer changes that teach something (a new metric, a concurrency pattern,
   a WASM target) over pure plumbing.
5. **A backtest is evidence, not truth.** Real capital is only ever risked on a
   strategy whose edge survives out-of-sample, and only after the same signals
   have run on *paper* long enough to show the backtest predicts live results.
   Paper-first is a rule, not a phase you can skip when impatient.
6. **Capital safety beats every feature.** Before any live order: hard position
   and loss limits, a manual kill switch, and durable position/PnL reconciliation.
   No exceptions, no "just this once with small size" without them.

---

## Horizon 1 — "Make it real" ✅ COMPLETE

The goal: go from a synthetic-data demo to something you'd actually point at a
real ticker and trust the numbers.

- ✅ **Developer experience.** `make dev` / `make test` / `make bindings`,
  `scripts/dev.sh` with health gate, GitHub Actions CI (cargo test + pytest +
  npm build). Committed `27d7efd`.
- ✅ **Real market data.** yfinance fetcher with split/dividend adjustment
  (`scripts/fetch_data.py`); 1257 bars each for AAPL, MSFT, GOOG, SPY, NVDA
  (2020-2024) in `data/datasets/`. `make fetch TICKER=X`. Committed `3d1daa4`.
- ✅ **Strategy breadth #1.** MACD (EMA-seeded crossover) and Bollinger Bands
  (population std-dev with `in_position` guard) — full Rust→PyO3→API→frontend
  pipeline, TDD throughout. Committed `a7fb138`.
- ✅ **Run persistence.** SQLite via `aiosqlite`; every `/backtest` and
  `/portfolio` call saves config + result and returns `run_id`. `GET /runs` +
  `GET /runs/{id}`. Frontend `RunHistoryPanel` with click-to-restore. Committed
  `3758b2a`.

## Horizon 2 — "Make it sharp" (the research workflow) ✅ COMPLETE

The goal: stop eyeballing single runs; support real strategy research.

- ✅ **Risk & portfolio analytics.** Pearson correlation matrix, annualised
  covariance, per-asset beta and contribution-to-risk, 21-bar rolling volatility,
  historical VaR/CVaR at 95 % and 99 %. Full panel in the frontend with heatmap
  and area chart. Committed `ae33962`.
- ✅ **Benchmark realism.** Optional `benchmark_symbol` field on `/portfolio`;
  returns a buy-and-hold NAV curve for any catalog dataset, overlaid as an orange
  dashed line on the equity chart. Committed `5145c1b`.
- ✅ **Parameter sweeps & optimization.** `POST /sweep` endpoint; Cartesian
  product over `ParamRange` inputs, Pydantic validation filters impossible combos
  (e.g. short_window ≥ long_window), results visualised as a colored bar chart
  (1 param) or 2D heatmap (2 params) with a "best combo" callout. Committed
  `c266881`.
- ✅ **Rebalancing.** Monthly, quarterly, and drift-threshold target-weight
  rebalancing via NAV-overlay in `portfolio_backtest.rs`; execution costs applied
  on each rebalance date; rebalance dates returned and shown as violet dashed
  reference lines on the equity chart. Committed `21232e2`.

> **Direction change (2026-06-23).** The goal has shifted from "better research
> tool" toward "trade real money, safely." The research-platform polish that used
> to be Horizon 3 (WASM, perf, hosted demo) is *not* on the path to trading and
> has been pushed to Horizon 5. The new Horizons 3 and 4 are the trading track:
> first make the backtest *trustworthy*, then go live on paper. See the decision
> log for why.

## Horizon 3 — "Make it trustworthy" (pre-trading hardening) ✅ COMPLETE

The goal: make the backtest a defensible predictor of live results. Nothing here
is optional before risking capital — a backtest you can't trust is worse than no
backtest, because it loses money with confidence.

- ✅ **Out-of-sample / walk-forward validation.** Sweeps optimise in-sample;
  trading a heatmap-selected parameter set is trading an overfit. New
  `POST /walkforward` endpoint with rolling folds: each fold tunes the param grid
  on a train slice, then scores the winner on the next (unseen) test slice. New
  "Walk-forward" frontend tab surfaces per-fold train-vs-test metrics so overfit
  is visible at a glance (test ≪ train). Committed `0a3d7cf`.
- ✅ **Execution realism that matches a broker.** Configurable `FillTiming`
  (`close` | `next_open`), defaulting to **next open** — a signal fills at the
  next bar's open with the existing slippage/commission, so backtest PnL is
  comparable to a live broker's. `close` stays available for reproducibility and
  A/B comparison. Threaded through engine → portfolio → sweep → walk-forward →
  API → UI. Bid/ask spread, market impact, and partial fills stay deferred until
  daily-bar, market-order trading actually hits those cases. Committed `ad7b4eb`.
- ✅ **Strategy validation pass.** Ran all six strategy families through
  `/walkforward` (5 rolling folds, `next_open` fills) across all 5 catalog
  assets. Two strategies survive out-of-sample across the full catalog: **RSI**
  (test Sharpe +1.18, positive on 5/5 assets, 80 % of folds) and **Bollinger
  Bands** (test Sharpe +1.13, positive on 5/5 assets). EMA and MACD are
  eliminated; WMA and SMA are conditional (AAPL/SPY only). Full findings and
  trading shortlist in `docs/strategy-validation.md`.

## Horizon 4 — "Go live (paper first)" (the trading track)

The goal: a daily loop that turns a strategy signal into a real (initially paper)
order, with the state and safety a live system needs. Gated hard behind paper
trading — see guiding principles 5 and 6.

- ✅ **Broker abstraction + dry-run loop.** `latest_signal` Rust primitive
  (PyO3) + `DryRunBroker` + idempotent position ledger (`positions` +
  `order_intents` tables) + `POST /trade/tick` API endpoint. The whole
  data→signal→order-intent→persistence loop runs with zero financial risk;
  re-ticking while already long is a no-op. 18 tests green. Committed
  `cc05ec9`.
- **Live data + scheduler.** A scheduled job that refreshes bars into the catalog
  (extend `scripts/fetch_data.py`) and runs the strategy on the *latest* bar to
  emit a target signal. Move from file-replay to a wall-clock loop.
- **Order management + reconciliation.** Translate target weights → orders, submit
  to the paper broker, track fills/rejections/partials, persist positions and
  cash, and reconcile broker-reported holdings against our own ledger. Most of the
  real risk lives here.
- **Risk guardrails + kill switch.** Max position size, max daily loss, and a
  manual halt. Non-negotiable before real capital (principle 6).
- **Paper soak.** Run on paper for weeks–months; compare live paper fills against
  what the backtester predicted for the same signals. Divergence is the reality
  check — and the deepest validation the engine can get. This is calendar time
  that can't be compressed.
- **Go live, small.** Only after the soak: flip the `Broker` adapter to live with
  minimal size, guardrails on.

## Horizon 5 — "Make it fast and shareable" (research platform, deprioritised)

Was Horizon 3. Genuine improvements to the *research tool*, but orthogonal to
trading — pursue only when something concretely demands them, after the trading
track is underway.

- **Performance.** Parallelize multi-asset and sweep runs with rayon; stream
  large datasets instead of loading whole-file; benchmark with criterion and
  guard against regressions. (Note: at current data sizes — ~1,257 bars × a few
  assets — this is premature; revisit when a real bottleneck appears.)
- **WASM core.** Compile the Rust engine to WebAssembly so single-asset backtests
  can run client-side with zero backend round-trip — a strong educational and
  latency win.
- **Persistence & deployment.** Database-backed run history, Dockerized stack,
  a hosted demo, and (if multi-user) auth. Shareable run permalinks.
- **Execution-model depth (remainder).** Bid/ask spread, market impact, partial
  fills, limit/stop order types — add as live trading actually encounters them.

## Horizon 6 — "Make it deep" (research-grade, optional)

Was Horizon 4. Stretch ideas, pursued only if the project wants to go further:

- Intraday / multi-timeframe and multi-currency support.
- Factor models and attribution (Fama-French style exposures).
- Pairs / statistical-arbitrage and other multi-asset strategies that need
  cross-asset signals (the current engine runs each asset independently).
- A small strategy DSL or plugin interface so strategies can be authored without
  touching the core.
- Monte Carlo simulation of return paths; scenario / stress testing.

---

## Cross-cutting, always-on

- **Testing.** Keep TDD; add property-based tests (proptest) for the engine and
  metrics; fuzz the CSV/JSON parsers.
- **Docs.** Per-module rustdoc, an architecture doc, and a strategy-authoring
  guide. Keep the README runnable-from-clean-clone.
- **DX hygiene.** Lockfile discipline, pinned toolchains, reproducible setup.

---

## Decision log (why, not just what)

- **Engine independence per asset.** v1 portfolio runs each asset through its own
  sub-portfolio and aggregates. Cross-asset strategies (pairs, risk parity
  rebalancing) will need a shared-clock engine — a deliberate Horizon 2/4 break.
- **Data lives on disk, resolved in Python.** Keeps the Rust core pure. A live
  fetcher writes CSVs into the existing catalog rather than calling the network
  from Rust.
- **No rebalancing in v1.** Static buy-and-hold-of-allocation was the smallest
  correct portfolio; rebalancing is the first Horizon 2 item precisely because
  it's the most-requested realism gap.
- **Trading reprioritised over research polish (2026-06-23).** With Horizons 1–2
  done, the goal shifted from "richer research tool" to "trade real money." Live
  trading is a different system class (live data, broker, order management,
  reconciliation, a wall-clock loop, kill switch) — not the next increment of the
  backtester — so it became its own track (Horizon 4) rather than an item under
  the old platform horizon. The research-platform work (WASM, perf, hosted demo)
  was demoted to Horizon 5: real, but orthogonal to trading.
- **Trustworthiness gates trading (2026-06-23).** A backtest is only evidence of
  an edge; an unvalidated or unrealistic one loses money with confidence. So
  out-of-sample/walk-forward validation and fill-at-next-bar execution realism
  were pulled out of the deferred horizons into Horizon 3 as hard prerequisites,
  ahead of any live order. "Fill at close" and per-asset independence are fine for
  research but must be understood as simplifications before trading on them.
- **Paper-first, not optional.** The backtester's own predictions are validated by
  running the same signals through paper trading and watching them agree. That
  makes a paper soak both a safety gate and the deepest engine-correctness test —
  hence it's a required step (principle 5), not a phase to skip when impatient.
