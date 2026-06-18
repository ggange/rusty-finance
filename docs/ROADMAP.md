# rusty-finance — Roadmap

A living document for the long run. The short-run, actively-worked plan lives in
issues / the current working session; this file is the map, not the turn-by-turn.

Last updated: 2026-06-18

---

## Where we are today

A working vertical slice, end to end:

- **Rust core** (`backtesting/`) — event-driven `BacktestEngine`, a `Portfolio`
  with cash/share accounting, execution costs and sizing rules, a `Strategy`
  trait, and an 8-metric analytics layer (total return, CAGR, vol, max drawdown,
  Sharpe, Sortino, win rate, trade count). Multi-asset orchestration
  (`portfolio_backtest.rs`) aggregates per-asset runs into one portfolio result.
- **Strategies** — two families: moving-average crossover (SMA/EMA/WMA) and RSI.
- **Bindings** (`backtesting-py/`) — PyO3 surface: `run`, `run_portfolio`, CSV helpers.
- **API** (`api/`) — FastAPI: `/health`, `/strategies`, `/backtest`, `/datasets`,
  `/datasets/{name}`, `/portfolio`.
- **Frontend** (`frontend/`) — React 18 + Vite 8, multi-asset portfolio form,
  results dashboard (equity/drawdown/price charts, per-asset drill-down).
- **Data** — synthetic CSVs loaded from disk (`CSVDataSource`); no live feed.

What this is **not** yet: it backtests buy-and-hold-of-allocation portfolios on
synthetic data, with no rebalancing, no real prices, no run persistence, no
parameter search, and no deployment story.

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

---

## Horizon 1 — "Make it real" (next few sessions)

The goal: go from a synthetic-data demo to something you'd actually point at a
real ticker and trust the numbers.

- **Developer experience.** One-command startup that can't hit the
  venv/maturin/uvicorn footgun. A `Makefile` (or `justfile`) with `make dev`,
  `make test`, `make bindings`; a `scripts/dev.sh` that activates `.venv`,
  rebuilds bindings, and launches API + Vite together with a health gate. CI
  (GitHub Actions) running `cargo test`, `pytest`, and `npm run build`.
- **Real market data.** A fetcher (yfinance / Stooq / Alpha Vantage) that
  populates `data/datasets/` with real OHLCV, plus dividend/split adjustment so
  returns are honest. Keep the on-disk CSV catalog contract intact.
- **Strategy breadth #1.** MACD and Bollinger Bands — both reuse the existing
  indicator scaffolding and exercise the strategy/param/validator path end to end.
- **Run persistence (minimal).** Save a backtest/portfolio run (config + result)
  to disk or SQLite; list and re-open past runs from the UI. Unblocks comparison.

## Horizon 2 — "Make it sharp" (the research workflow)

The goal: stop eyeballing single runs; support real strategy research.

- **Rebalancing.** Periodic (monthly/quarterly) and threshold-based target-weight
  rebalancing in `portfolio_backtest.rs`. This is the top "out of scope v1"
  follow-up and changes the portfolio from static allocation to a managed sleeve.
- **Parameter sweeps & optimization.** Grid search / walk-forward over strategy
  params, run in parallel (rayon) in the core. Surface a heatmap of a metric over
  the parameter space in the UI. Add overfitting guards (out-of-sample split).
- **Risk & portfolio analytics.** Correlation/covariance matrix, portfolio beta,
  rolling volatility, VaR/CVaR, and a contribution-to-risk breakdown per asset.
- **Benchmark realism.** Compare against a configurable benchmark series (e.g.
  SPY) rather than a synthesized buy-and-hold only.

## Horizon 3 — "Make it fast and shareable" (platform)

The goal: scale the engine and make runs reproducible/shareable.

- **Performance.** Parallelize multi-asset and sweep runs with rayon; stream
  large datasets instead of loading whole-file; benchmark with criterion and
  guard against regressions.
- **WASM core.** Compile the Rust engine to WebAssembly so single-asset backtests
  can run client-side with zero backend round-trip — a strong educational and
  latency win.
- **Persistence & deployment.** Database-backed run history, Dockerized stack,
  a hosted demo, and (if multi-user) auth. Shareable run permalinks.
- **Execution-model depth.** Bid/ask spread, market impact, partial fills,
  limit/stop order types — move beyond "fill at close."

## Horizon 4 — "Make it deep" (research-grade, optional)

Stretch ideas, pursued only if the project wants to go further:

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
