# rusty-finance — Roadmap

A living document for the long run. The short-run, actively-worked plan lives in
issues / the current working session; this file is the map, not the turn-by-turn.

Last updated: 2026-08-16

**Active horizon: 5 — "Make it defensible."** Horizon 4 (live trading) is
deliberately parked at simulation; see the 2026-08-16 decision-log entry.

---

## Where we are today

A fully real-data, multi-strategy backtesting and research stack, plus a
simulated trading loop, end to end:

- **Rust core** (`backtesting/`) — event-driven `BacktestEngine`, a `Portfolio`
  with cash/share accounting, execution costs and sizing rules, a `Strategy`
  trait, and an 8-metric analytics layer (total return, CAGR, vol, max drawdown,
  Sharpe, Sortino, win rate, trade count). Multi-asset orchestration
  (`portfolio_backtest.rs`) aggregates per-asset runs into one portfolio result,
  with optional NAV-overlay rebalancing (monthly, quarterly, drift-threshold)
  and gradient-projection weight optimisation onto the simplex. ~5,200 lines,
  132 tests, no dependencies beyond chrono/csv/serde.
- **Strategies** — four families / six variants: moving-average crossover
  (SMA/EMA/WMA), RSI, MACD (EMA-seeded crossover), and Bollinger Bands
  (population std-dev bands).
- **Risk analytics** (`risk.rs`) — Pearson correlation matrix, annualised covariance,
  per-asset beta and contribution-to-risk, 21-bar rolling volatility,
  historical VaR/CVaR at 95 % and 99 %.
- **Validation** (`walkforward.rs`) — rolling walk-forward folds; each fold tunes
  a param grid in-sample and scores the winner on unseen data.
- **Bindings** (`backtesting-py/`) — PyO3 surface: `run`, `run_portfolio`,
  `run_sweep`, `run_walkforward`, `latest_signal`, CSV helpers.
- **API** (`api/`) — FastAPI: backtest / portfolio / sweep / walk-forward /
  datasets / runs, plus the trading surface (`/trade/tick`, `/trade/plans`,
  `/trade/orders`, `/trade/reconcile`, `/trade/limits`, `/trade/killswitch`,
  `/trade/schedule`, `/trade/broker`, `/trade/soak`). ~2,900 lines, 216 tests.
- **Frontend** (`frontend/`) — React 18 + Vite, multi-asset portfolio form,
  results dashboard (equity/drawdown/price charts, risk analytics panel,
  per-asset drill-down), run history, parameter sweep tab (bar chart / 2D
  heatmap), walk-forward tab, trade console. Vitest coverage on hooks/lib/ui.
- **Trading loop** — signal → order intent → risk chokepoint → broker → ledger →
  reconciliation, on an APScheduler cron, against simulated brokers only. Every
  bundled broker reports `is_live = False`.
- **Data** — real OHLCV for AAPL, MSFT, GOOG, SPY, NVDA (2020-01 → 2026-07,
  yfinance, split/dividend-adjusted), incrementally refreshable. Synthetic
  fixtures for tests.
- **Persistence** — SQLite (`data/runs.db`): runs, positions, order intents,
  orders, venue books, trade plans, risk limits, kill switch.
- **Dev tooling** — `make dev` / `make test` / `make bindings` / `make refresh`,
  one-command startup with health gate, GitHub Actions CI (cargo test + pytest +
  Vitest + typed production build).

**What this is not yet.** Two honest gaps, and they are the whole reason
Horizon 5 exists:

1. **The results have no error bars.** Every metric the platform reports —
   Sharpe, CAGR, the walk-forward "test Sharpe," the sweep's best combo — is a
   point estimate from a short sample. A Sharpe of 1.18 measured over ~75 daily
   bars has a standard error large enough to contain zero. The platform
   currently presents point estimates with a confidence they have not earned,
   and the sweep's arg-max is uncorrected for the number of trials that produced
   it. Until that is fixed, the strategy conclusions in
   `docs/strategy-validation.md` are directional, not established.
2. **The strategy library is single-name technical analysis.** Every strategy
   signals off one asset's own price history, because the engine runs each asset
   independently. There is no cross-sectional ranking, no market-neutral
   construction, no relative-value signal — the class of thing systematic
   research actually does.

It is also not a *live* trading system, and deliberately no longer trying to be
one: no broker credentials, no live venue, no real capital. See Horizon 4.

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
7. **No number without an interval, no arg-max without a correction.**
   *(Added 2026-08-16.)* A point-estimate Sharpe from a short sample is a
   coin-flip dressed as a finding, and the best cell of an N-cell sweep is a
   biased estimator by construction. Every performance figure the platform
   reports should carry an uncertainty measure, and every selected-by-search
   result should be discounted for the size of the search. Where a correction
   makes a previously-reported result weaker, the weaker result is the one that
   ships — reporting that a claim did not survive is the point of measuring.

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
> has been pushed to Horizon 6. The new Horizons 3 and 4 are the trading track:
> first make the backtest *trustworthy*, then go live on paper. See the decision
> log for why.
>
> **Superseded in part (2026-08-16).** The trading destination was reached in
> simulation and parked there; the *trustworthiness* thesis of Horizon 3 turned
> out to be the more valuable half and is resumed as Horizon 5.

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

  > **Follow-up owed (flagged 2026-08-16).** These are point estimates over
  > ~75-bar test folds, reported without confidence intervals, and the per-fold
  > winner is an uncorrected arg-max over the param grid. The *ranking* is
  > probably informative; the *magnitudes* are not established, and "test Sharpe
  > exceeded train Sharpe" is evidence of noise, not of edge. Horizon 5 item A
  > re-runs this pass with intervals and multiple-testing corrections, and
  > `docs/strategy-validation.md` carries the caveat until it does.

## Horizon 4 — "Go live (paper first)" ⏸️ PARKED AT SIMULATION

The goal *was* a daily loop that turns a strategy signal into a real (initially
paper) order, with the state and safety a live system needs.

**Status (2026-08-16): the loop is built, validated in simulation, and stops
there by choice.** Everything through soak reporting shipped. The remaining
three items — real broker adapter, paper soak, go live — are **withdrawn from
the active plan**, not merely deferred. Reasoning in the decision log; the short
version is that they cost months of calendar time, produce no artifact that
demonstrates anything the simulated loop doesn't already demonstrate, and attach
live-order code to a public repository for no research benefit.

Delivered:

- ✅ **Broker abstraction + dry-run loop.** `latest_signal` Rust primitive
  (PyO3) + `DryRunBroker` + idempotent position ledger (`positions` +
  `order_intents` tables) + `POST /trade/tick` API endpoint. The whole
  data→signal→order-intent→persistence loop runs with zero financial risk;
  re-ticking while already long is a no-op. 18 tests green. Committed
  `29d1a46`.
- ✅ **Live data + scheduler.** Incremental fetch (`fetch_incremental` /
  `make refresh`) appends only bars newer than what's on disk, deduping on date
  so the trailing bar picks up post-close revisions; a failed fetch never
  destroys history. Trading plans are now persisted (`trade_plans` table,
  `POST/GET/DELETE /trade/plans`) because a scheduled run has no request body to
  read items from. `api/scheduler.py` runs an in-process APScheduler on a
  mon-fri 16:30 America/New_York cron: refresh every plan symbol, then tick each
  enabled plan through `DryRunBroker`. `GET /trade/schedule` exposes cron config
  / next run / last run; `POST /trade/schedule/run` triggers a cycle manually.
  The catalog was refreshed off its 2024-12-30 stale point to 2026-07-31
  (+396 bars/symbol). 35 new tests (120 Python total).
- ✅ **Risk guardrails + kill switch.** `api/risk.py` is the single chokepoint
  every order passes through before reaching a broker. Limits (`max_position_value`,
  `max_daily_loss`, `max_daily_orders`) are stored per plan and layered
  field-by-field over a global fallback row; a durable kill switch survives
  restart. Two deliberate asymmetries: limits constrain entries but never exits
  (blocking a sell would strand capital), while the kill switch halts everything
  including exits (a halt that still traded wouldn't be a halt). Rejections are
  logged with a `rejected:` status, leave the ledger untouched, and don't consume
  the daily order budget. Endpoints: `GET/POST/DELETE /trade/limits`,
  `GET/POST /trade/killswitch`. 40 new tests (160 Python total).

  > **Known gap at the time, since closed.** An unconfigured install was
  > *permissive* — no limits stored meant unlimited, with only a boot warning.
  > Acceptable while `DryRunBroker` was the only broker; closed by the
  > `is_live` fail-closed check in the next item.
- ✅ **Order management + reconciliation.** `Broker.submit` now returns a
  `BrokerOrder` (`status`, `filled_qty`, `avg_fill_price`) rather than a status
  string, with `get_order` polling and `list_positions`. Orders persist to an
  `orders` table separately from intents — a rejected intent never becomes an
  order, and an order can outlive the tick that created it. **The ledger follows
  what actually filled, never what was requested**: a partial buy records only
  the filled shares at the fill price, and `sync_open_orders` tops up newly
  filled quantity at the start of each tick without double-counting.
  `reconcile()` compares venue holdings against the ledger per symbol over the
  union of both sets, and runs on every tick. `SimulatedPaperBroker` models
  partial fills, rejections and adverse slippage so all of this is exercised
  before a vendor is chosen. Endpoints: `GET /trade/orders`,
  `GET /trade/reconcile`. 31 new tests (192 Python total).

  The fail-closed gap flagged above is now closed: brokers declare `is_live`,
  and a live one with `max_position_value` or `max_daily_loss` unset has every
  order refused in both directions.
- ✅ **Broker selection + persistent paper venue.** `make_broker()` is the one
  place orders are routed from, driven by `RUSTY_FINANCE_BROKER`
  (`dry_run` | `paper_sim`), with slippage and fill-ratio tunable by env; an
  unknown name falls back to `dry_run` with a warning rather than failing open.
  `PersistentPaperBroker` keeps the venue's own books in
  `venue_positions` / `venue_orders`, derived independently of our ledger, so a
  restart mid-soak doesn't manufacture phantom drift. Neither shipped broker is
  live. Endpoint: `GET /trade/broker`.
- ✅ **Soak reporting.** `GET /trade/soak` compares realized fills against the
  signal price, in basis points, signed so positive always means "worse than
  assumed" for both sides — plus fill rate, rejections and realized PnL.
  24 new tests (216 Python total).

  Exercised by replaying 90 trading days of MSFT + NVDA (RSI-14, 8 bps
  configured slippage) through the live loop: 22 orders, 100 % fill rate,
  **zero reconciliation drift across all 90 ticks**, ledger and venue agreeing
  at the end.

  > **Correction (2026-08-16): the slippage figure was not evidence of
  > anything.** This bullet previously read "mean slippage measured at exactly
  > 8.00 bps" and offered it as the backtest validating itself. Two defects,
  > found in code review:
  >
  > 1. **The measurement is circular.** The simulated broker fills at
  >    `intent.price × (1 + slippage)`, and the soak measures the gap between
  >    the fill and `intent.price`. It can only ever return the configured
  >    constant. Recovering *exactly* 8.00 bps from an 8 bps input was the tell:
  >    a real measurement never lands precisely on its own input.
  > 2. **It compares against the wrong reference price.** `intent.price` is the
  >    last bar's **close**, but the engine's default fill timing is
  >    `next_open`. So the metric omits the overnight gap — which for a loop
  >    firing at 16:30 after the close is the largest single component of the
  >    real modelling error, and the whole reason `next_open` was made the
  >    default in Horizon 3.
  >
  > What the replay *did* establish stands: loop stability over 90 ticks,
  > scheduler correctness, and zero reconciliation drift between ledger and
  > venue. Those are real and were worth having. What it did not establish is
  > anything about fill realism. Fixing the reference price to the next bar's
  > open makes the metric meaningful, but it still cannot validate fills against
  > a simulator whose fills we define — that genuinely needs a real venue, which
  > is exactly what the withdrawn vendor item would have supplied. Principle 7
  > applies to the platform's own self-measurement, not just to strategy Sharpes.

Withdrawn (was: next up):

- ❌ **Real broker adapter.** Would need credentials handling, an account, and a
  vendor API shape. Implementing `Broker` against a real venue exercises no
  design the simulated brokers don't already exercise — `SimulatedPaperBroker`
  already models partial fills, rejections and adverse slippage, and the
  `is_live` fail-closed path is already tested. What a real venue would add is
  *true* fill behaviour, which is only worth having if the intent is to trade.
- ❌ **Paper soak.** Weeks-to-months of calendar time. The 90-day replay already
  demonstrated loop stability, scheduler correctness, guardrails and zero
  reconciliation drift.
- ❌ **Go live, small.** Out of scope. This repository is research and
  educational software; the README disclaimer is a design statement, not a
  placeholder.

If the goal ever changes back to trading real capital, these three items are
still the right next steps in this order, and guiding principles 5 and 6 still
gate them hard. Nothing built here needs to be undone to resume.

## Horizon 5 — "Make it defensible" ⬅️ ACTIVE

The goal: close the two gaps named in "What this is not yet." Horizon 3 asked
whether a strategy survives out-of-sample and answered with a single number per
fold. Horizon 5 asks how much of that number is signal — and widens the strategy
class beyond what a single price series can express.

Ordered by value; A is the highest-leverage work in the repository.

### A. Statistical rigour — uncertainty and multiple-testing correction

Pure compute, so it belongs in the Rust core, and it is where the platform
currently overstates what it knows. Each item is a metric with a paper behind it.

- ✅ **Correctness pass on the existing metrics (2026-08-16).** Prerequisite for
  everything below: no value in bootstrapping a confidence interval around a
  biased estimator. A code review found and this pass fixed three defects, each
  red→green with a test naming the failure:
  - **Historical VaR/CVaR selected the wrong order statistic.** `floor(q·n)`
    sits one observation inside the body of the distribution, so the reported
    tail loss was always too mild — and with exactly 5 losing days in 100 it
    landed on the first *winning* day and reported a **positive** VaR-95. Now
    `ceil(q·n) − 1`. The old test pinned the wrong values, so it was rewritten
    rather than kept. CVaR-99's small-sample collapse (a 1 % tail is one day for
    `n ≤ 100`, making CVaR-99 numerically equal to VaR-99) is now documented and
    pinned as a known property.
  - **CAGR annualised by the observation count instead of the return periods.**
    An `n`-point curve spans `n − 1` returns. Small over years, material over a
    walk-forward fold — and `cagr` is a selectable fold-ranking metric, so the
    error reached parameter selection. Fixed in `Metrics` and `Benchmark`
    together, since the two are displayed side by side as strategy vs
    buy-and-hold and must share a convention. The residual bar-count assumption
    (wrong for non-daily candles) is now stated in the rustdoc.
  - **RSI was not computing Wilder's RSI, and read a flat series as maximally
    overbought.** Three defects in the project's primary surviving strategy.
    (a) `avg_loss == 0` returned RSI 100 — but a *flat* window has no upside
    either, and no movement is neutral (50), not overbought; a halted or thinly
    traded symbol with repeated closes emitted **Sell**. (b) The first bar was
    seeded with a synthetic zero change, so the first real RSI mixed `period − 1`
    observations with one fabricated one, and the documented warm-up did not
    match the code. (c) The "Wilder smoothing" re-seeded from the oldest value
    in a rolling `period`-length window and folded forward on every call, making
    the result a function of the window's contents rather than of the whole
    history — neither Wilder's RSI nor a simple-average RSI, despite the doc
    claiming Wilder. Now a genuine running Wilder average: seeded once from the
    simple mean of the first `period` changes, then advanced as
    `(avg × (period − 1) + change) / period`. A test pins the defining property
    — two series with identical trailing changes but different histories now
    report different RSI, which a windowed average could not do.

    **This changes RSI's output on real data**, so every RSI figure in
    `docs/strategy-validation.md` predates the strategy now implemented. Noted
    there.
  - ⚠️ **Reverted: excluding zero-variance assets from the weight solve.** The
    review found that the degeneracy guard fires only when *every* covariance
    diagonal is zero, so one flat asset reads as riskless and min-variance loads
    up on the asset it has least information about. That is a real hazard and
    the finding was correct — but the proposed fix, treating a zero-variance
    column as unusable, is **wrong for this engine**, and shipping it caused a
    regression that `test_min_variance_beats_equal_weight_at_every_solve`
    caught.

    The reason: `optimize_weights` cannot distinguish "the window holds no
    information about this asset" from "the strategy was in cash all window".
    Both produce an identically flat NAV series, and cash genuinely *is*
    riskless. Excluding flat columns therefore breaks min-variance's defining
    guarantee — predicted volatility never above equal weight, which is always
    a feasible point — whenever a strategy sits out a window. Correcting RSI
    made exactly that happen, which is how the latent break surfaced.

    Reverted, with the hazard pinned by
    `a_single_flat_asset_is_treated_as_riskless` and the invariant it must not
    break pinned by `min_variance_never_predicts_more_risk_than_equal_weight`.
    The real fix belongs upstream in `portfolio_backtest.rs`, where per-asset
    first-bar dates are known and the two cases *are* distinguishable — carried
    forward as part of this item.

- ✅ **Walk-forward warm start and tie reporting (2026-08-16).** The two defects
  from the same review that bore directly on this item's rewrite of
  `strategy-validation.md`:
  - **Test slices cold-started their indicators.** The winning combo was re-run
    on the test slice from scratch, so every test window paid a second warm-up
    and lost its first `period` bars — and when the test slice was shorter than
    the indicator's period, it could not trade *at all* and reported an all-zero
    metric set as an out-of-sample score, indistinguishable from a strategy that
    legitimately found nothing to do. The chosen combo now runs across the whole
    fold and is scored from the first test bar onward, so its state is what a
    live deployment would hold on that date. No look-ahead: every bar it has seen
    precedes the scored window. A direct A/B test pins the difference — the same
    spec on the same 15 bars trades 0 times cold and >0 times warm-started.

    **This changes the reading of the existing validation results.** A 20-period
    Bollinger band on a 75-bar test fold was burning a quarter of the window to
    warm-up, which is a more plausible explanation of that strategy's many
    "no-signal folds" than the selectivity the doc currently claims.
  - **Ties resolved silently to grid index 0.** Selection was strict `>` from
    `NEG_INFINITY`, so equal scores kept the first combo — and ties are the
    common case, because every strategy that never fires scores exactly 0.0.
    Folds now report `tied_candidates`, surfaced in the walk-forward table as an
    "N-way tie" badge, so a fold that did not really select its parameters says
    so. At an exact tie a combo that traded now beats one that did not; beyond
    that the tie-break is still arbitrary, which is the reason for reporting the
    count rather than hiding it.

  - **`/portfolio/optimize` estimated covariance across calendar-misaligned
    series.** Returns were built per dataset and passed positionally, so the Rust
    side truncated to the shortest by taking the *head*: a 2,000-bar history
    paired its **oldest** 499 returns with a 500-bar history's most recent ones —
    a covariance between two different periods, which measures nothing. Two
    datasets a year apart returned `200 OK` with a confidently computed answer.
    `lookback` had the same defect on the other axis, slicing each series from its
    own end, so a stale CSV beside a freshly refreshed one came out offset by the
    gap — routine, since the incremental fetcher only refreshes symbols named by
    a trade plan. Returns are now keyed by date, intersected across datasets, and
    `lookback` counts back along that shared axis; a non-overlapping request is a
    422 instead of a number. The response reports the `window` actually estimated
    over, which is generally narrower than any single dataset's range.

    `covariance()` keeps accepting unequal lengths — it is reachable from the FFI
    boundary, where a panic is worse than a defensible guess — but now falls back
    to the most recent overlap rather than the earliest, and its rustdoc states
    that alignment is the caller's job. Head alignment was the worst of the
    available guesses.

  Not yet fixed, carried forward: the upstream half of the zero-variance fix
  above. It does not block item A.

- **Confidence intervals on every performance metric.** Stationary bootstrap
  (Politis & Romano 1994) over the return series, so autocorrelation and
  volatility clustering are preserved in the resamples — an IID bootstrap would
  produce intervals that are too tight. Report Sharpe, CAGR, max drawdown and
  Sortino as estimate + interval throughout the API and UI. Cross-check the
  Sharpe interval against Lo (2002)'s analytic autocorrelation-adjusted standard
  error as a unit-test oracle.
- **Deflated Sharpe Ratio** (Bailey & López de Prado 2014) on sweep output.
  Every input is already available: the number of trials, the variance of Sharpe
  across trials, the sample length, and the winner's skew and kurtosis. Surface
  it directly beside the "best combo" callout, so the UI element that currently
  invites overfitting is the one that reports its own bias.
- **Probability of Backtest Overfitting** via combinatorially-symmetric
  cross-validation (Bailey, Borwein, López de Prado & Zhu 2017). Answers "what
  fraction of the time does my in-sample winner underperform the median
  out-of-sample?" — a single number per research process, which is the honest
  unit of measurement for a parameter search.
- **Combinatorial purged cross-validation** (López de Prado, *AFML* ch. 12) as
  an upgrade path from rolling walk-forward. 5 rolling folds give 5 test paths;
  CPCV over the same data gives hundreds, which is what makes PBO estimable in
  the first place. Purging and embargo matter little for same-bar daily signals
  but are the correct construction and become load-bearing the moment a
  multi-bar holding period or label horizon appears.
- **Re-run the Horizon 3 validation pass** with all of the above and rewrite
  `docs/strategy-validation.md`. Expect conclusions to weaken. Per principle 7,
  the weaker conclusion is the deliverable — including, if it comes to it,
  "neither surviving strategy is distinguishable from zero at this sample size."
- **Transaction-cost sensitivity.** Sweep Sharpe as a function of slippage and
  commission rather than fixing them at one value. A strategy whose edge dies at
  15 bps is a different object from one that survives 50, and only one of them is
  a finding.

### B. Cross-sectional engine — the shared clock

The largest remaining architectural gap, and the one the decision log has been
pointing at since v1: the engine runs each asset independently, so no strategy
can condition on another asset's price at the same bar.

- **Shared-clock engine.** A bar loop over a date-aligned panel of assets rather
  than N independent loops, with explicit handling of missing bars and
  non-overlapping histories. The existing single-asset path stays as the
  degenerate case, and the allocation/rebalancing overlay is unaffected — as the
  decision log notes, weight solving never needed the shared clock; *signalling*
  does.
- **Cross-sectional strategies.** Rank the universe each bar and trade the
  extremes: cross-sectional momentum, short-horizon reversal. This is the class
  of strategy systematic research actually runs, and it retires the "single-name
  technical analysis" characterisation.
- **Dollar-neutral long/short construction.** Requires `Portfolio` to express a
  short position, which today it cannot — a real change, and the reason the
  optimiser projects onto the simplex. Scope carefully: shorting brings borrow
  cost and margin accounting, and a fake short is worse than none.
- **Cointegration pairs.** Engle-Granger / ADF on a spread, trade deviations.
  The canonical shared-clock strategy, and a good forcing function for the
  engine design.

### C. Performance story with numbers

The repository asserts "Rust core" as a reason to care and never quantifies it.
Item A's CPCV workload also creates the first genuine compute demand in the
project, which retires the standing "premature optimisation" caveat honestly.

- **criterion benchmarks** on the engine hot path, metrics, the optimiser, and a
  full CPCV run. Committed baselines, regression-guarded in CI.
- **rayon parallelism** across sweep cells, walk-forward folds and CPCV paths —
  embarrassingly parallel, and a genuinely instructive Rust concurrency exercise
  per principle 4.
- **A reference implementation to benchmark against.** A small, honest
  numpy/pandas version of the same backtest, so "why Rust" has an answer with a
  ratio in it. Publish the table in the README: ns/bar, sweep cells/sec, CPCV
  paths/sec, and the speedup. This is the number people remember, and a fair
  comparison is worth more than a flattering one.

### D. Data breadth

Cheap, and it strengthens every claim item A produces.

- **A wider universe.** Five hand-picked US megacaps, chosen in 2026 for a
  2020-2026 window, is a selection-biased sample of one regime. An index
  constituent list is the fix.
- **A pre-2020 window.** Extend history to cover 2008 and 2000, so drawdown and
  tail metrics see an actual crisis rather than one V-shaped recovery.
- **Survivorship bias, named.** A current-constituents list is itself biased;
  document it in the data README even where it isn't fixed. Knowing which biases
  remain is worth more than pretending none do.

### E. Cross-cutting quick wins

- **proptest** on the engine and metrics: equity curve monotone in a monotone
  price series, cash + holdings conserved across fills, metrics invariant to
  scaling initial cash.
- **`docs/ARCHITECTURE.md`** — the layer boundaries and why they sit where they
  do. Currently implicit in the README's two ASCII diagrams.
- **Fix the README's framing** of "out-of-sample Sharpe exceeds in-sample in
  four of five folds." As written it reads as a strength; it is a symptom of
  fold samples too small for the comparison to carry information. Principle 7
  applies to the front page first.

## Horizon 6 — "Make it fast and shareable" (research platform, deprioritised)

Was Horizon 3, then Horizon 5. Genuine improvements to the *research tool*, but
orthogonal to defensibility — pursue only when something concretely demands them.
Note that the performance half has been promoted into Horizon 5 item C, where it
now has a concrete workload behind it.

- **WASM core.** Compile the Rust engine to WebAssembly so single-asset backtests
  can run client-side with zero backend round-trip — a strong educational and
  latency win.
- **Streaming large datasets** instead of loading whole-file, once the wider
  universe of Horizon 5 item D makes whole-file loading actually hurt.
- **Persistence & deployment.** Dockerized stack, a hosted demo, and (if
  multi-user) auth. Shareable run permalinks.
- **Execution-model depth (remainder).** Bid/ask spread, market impact, partial
  fills, limit/stop order types — add as the research actually encounters them.

## Horizon 7 — "Make it deep" (research-grade, optional)

Was Horizon 4, then 6. Stretch ideas, pursued only if the project wants to go
further. Note that pairs / statistical arbitrage moved up into Horizon 5 item B,
since it is the natural forcing function for the shared-clock engine.

- Intraday / multi-timeframe and multi-currency support.
- Factor models and attribution (Fama-French style exposures) — a natural
  successor to the cross-sectional engine, since factor attribution needs the
  same date-aligned panel.
- A small strategy DSL or plugin interface so strategies can be authored without
  touching the core.
- Monte Carlo simulation of return paths; scenario / stress testing.

---

## Cross-cutting, always-on

- **Testing.** Keep TDD; property-based tests (proptest) for the engine and
  metrics; fuzz the CSV/JSON parsers.
- **Docs.** Per-module rustdoc, an architecture doc, and a strategy-authoring
  guide. Keep the README runnable-from-clean-clone.
- **DX hygiene.** Lockfile discipline, pinned toolchains, reproducible setup.
- **Honest reporting.** When a correction weakens a previously-published result,
  update the published result. The decision log records reversals; the metrics
  docs should too.

---

## Decision log (why, not just what)

- **Engine independence per asset.** v1 portfolio runs each asset through its own
  sub-portfolio and aggregates. Cross-asset strategies (pairs, risk parity
  rebalancing) will need a shared-clock engine — a deliberate Horizon 2/4 break.
  - **Amended.** Weight optimization, including risk parity re-solved at every
    rebalance date, turned out *not* to need the shared clock. Rebalancing was
    already a NAV overlay that answers "at this date, what are the target
    weights?", so a solver only had to supply different targets. What genuinely
    needs a shared clock is cross-asset *signalling* — asset A's entry depending
    on asset B's price at the same bar, as in pairs trading. Allocation and
    signalling were conflated in the original note.
  - **Now scheduled.** The shared clock is Horizon 5 item B. The deferral held
    for as long as every strategy was single-name; it stops holding the moment
    the goal is a strategy class that isn't.
- **Weight solving never sees the future.** A static solve could have optimized
  over all history and allocated from day one; that is the textbook framing and
  it is look-ahead. Instead `static` spends a warm-up window estimating and
  switches once it is full, and `dynamic` re-solves from a trailing window. Both
  are walk-forward by construction, matching how strategy parameters are already
  validated.
- **Levels, not multipliers, in the rebalance overlay.** The overlay used to
  accumulate multiplicative rebalance ratios. That is fine while weights stay
  positive, but an optimizer may legitimately drop an asset to ~0 and later want
  it back, which needs `target / ≈0` — infinity, then NaN across the whole
  equity curve. Tracking each asset's NAV level directly re-funds cleanly.
- **Long-only, because the engine is.** Every solver projects onto the simplex.
  Unconstrained mean-variance would return shorts, and `Portfolio` cannot express
  one — the backtest would silently not be the portfolio that was solved for.
  (Horizon 5 item B revisits this deliberately: shorting is a real change to
  `Portfolio`, with borrow cost and margin, not a constraint to relax.)
- **Optimizing is cheap; being right is not.** A solve costs 6–15 µs, so
  per-rebalance optimization is free next to the engine (78 solves add ~1 ms to a
  9 ms run). The constraint was never compute. On the real catalog, dynamic
  max-Sharpe produced the *highest* return (193% vs 149%) and a *worse* Sharpe
  than equal weight (0.77 vs 0.80), with a −34% drawdown against −28%. Principle
  5 applies to weights exactly as it does to strategy parameters.
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
  was demoted: real, but orthogonal to trading.
- **Trustworthiness gates trading (2026-06-23).** A backtest is only evidence of
  an edge; an unvalidated or unrealistic one loses money with confidence. So
  out-of-sample/walk-forward validation and fill-at-next-bar execution realism
  were pulled out of the deferred horizons into Horizon 3 as hard prerequisites,
  ahead of any live order. "Fill at close" and per-asset independence are fine for
  research but must be understood as simplifications before trading on them.
- **Vendor deferred, soak started on the simulator (2026-08-02).** With the
  order lifecycle, guardrails and reconciliation done, the next step was a real
  broker adapter — but choosing a vendor commits to credentials, an account and
  an API shape, and none of that is needed to learn whether the *loop* is
  stable. So the soak starts against `PersistentPaperBroker` with realistic
  slippage. It validates loop stability, the scheduler, guardrails,
  reconciliation and the ledger over calendar time; it explicitly does **not**
  validate real fills, venue rejections or true slippage. Those need a real
  venue, and the honest reading is that the simulated soak is a prerequisite
  for the vendor work, not a substitute for it.
- **Paper-first, not optional.** The backtester's own predictions are validated by
  running the same signals through paper trading and watching them agree. That
  makes a paper soak both a safety gate and the deepest engine-correctness test —
  hence it's a required step (principle 5), not a phase to skip when impatient.
- **Trading parked at simulation; rigour resumed (2026-08-16).** The vendor
  deferral of 2026-08-02 is made permanent, and the paper soak and go-live steps
  are withdrawn rather than merely postponed. Three reasons, in order of weight:

  1. **The remaining trading work is plumbing, not design.** The interesting
     problems — idempotency, ledger-follows-fills, reconciliation over the union
     of both position sets, a single risk chokepoint, fail-closed on `is_live`,
     a kill switch that halts exits too — are all solved and tested. A vendor
     adapter adds credentials handling and one more `Broker` implementation
     against an interface that already has three. It exercises no new design.
  2. **The paper soak is calendar time, and the thing it would measure is
     already measured well enough to act on.** 90 replayed trading days gave
     100 % fill rate, slippage matching configuration to 0.01 bps, and zero
     reconciliation drift across every tick. A live paper soak would add true
     venue behaviour — worth having only if the intent is to trade real capital,
     which it is not.
  3. **A backtest with no error bars is a more serious defect than a backtest
     with no broker.** Horizon 3 declared the engine "trustworthy" on the basis
     of point-estimate Sharpes over ~75-bar folds, selected by uncorrected
     arg-max over a parameter grid. That is the same class of error the horizon
     was created to prevent, one level up: walk-forward removed the *parameter*
     overfit and left the *selection* and *sampling* problems untouched. Going
     live on top of that would have been trading a number nobody had bounded.

  So Horizon 5 attacks the measurement instead, and adds principle 7 to make the
  rule explicit rather than relearned. The trading loop stays in the repository,
  fully working, as what it honestly is: a simulated implementation of the
  state and safety machinery a live system needs. Nothing needs undoing if the
  goal reverts — the three withdrawn items are still the right next steps, in
  the same order, behind the same principle 5 and 6 gates.
- **Strategy breadth was never the gap; strategy *class* is (2026-08-16).** The
  obvious response to "six strategies is thin" is a seventh. But all six answer
  the same question — what does this asset's own price history imply — because
  the engine can only ask that one. Another single-name indicator adds a row to
  the validation table and no new capability. The shared clock (Horizon 5 item B)
  changes what questions can be asked at all, which is why it outranks breadth
  even though it costs more. Principle 3 applies: a small number of strategies
  that span a real class beats many that span one.
