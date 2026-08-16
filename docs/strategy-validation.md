# Strategy Validation — Walk-Forward Results

> **Purpose**: Close Horizon 3's final bullet. Before any live or paper order,
> each strategy must show a positive out-of-sample Sharpe across the catalog
> assets. Strategies that only look good in-sample are overfit and will lose
> money with confidence.

Run date: 2026-06-23  
Engine: `next_open` fill timing, $10 k initial cash, $1 commission, 0.1 % slippage  
Method: 5 rolling walk-forward folds, `train_frac = 0.70`, optimised on Sharpe ratio  
Catalog: AAPL, MSFT, GOOG, NVDA, SPY (2020-01-02 → 2024-12-30, 1 257 bars each)  
Each fold: ~175 train bars (~8–9 months), ~75 test bars (~3–4 months)

---

## ⚠️ Read this before the numbers (added 2026-08-16)

**Every Sharpe in this document is an unbounded point estimate, and the sample is
too small for the magnitudes to mean what they appear to mean.** This section was
added after the fact, when it became clear the original write-up reported figures
with more confidence than the data supports. The findings are left unedited below;
this is the correction, not a rewrite.

**How wide the error bars are.** For a Sharpe estimated from `n` returns, the
approximate standard error is `√((1 + SR²/2) / n)` in periodic units (Lo 2002;
Jobson & Korkie 1981). For a ~75-bar test fold, annualised:

| Quantity | Estimate | Approx. SE | Approx. 95 % interval |
|---|---|---|---|
| A single fold's test Sharpe | 1.18 | **±1.84** | **[−2.4, +4.8]** |
| 25-fold average, *if folds were independent* | 1.18 | ±0.37 | [+0.46, +1.90] |
| 25-fold average, treating each calendar window as one observation | 1.18 | ±0.82 | **[−0.43, +2.79]** |

Three consequences, in order of how badly they undercut the original reading:

1. **A single fold's Sharpe is nearly uninformative.** An interval spanning −2.4
   to +4.8 cannot distinguish a good strategy from a bad one. Every per-fold
   number in the tables below should be read as a data point, never a result.
2. **Whether the headline average is significant depends entirely on how many
   independent observations 25 folds represent — and it is closer to 5 than 25.**
   The five assets are four US megacaps plus the index that contains them, with
   pairwise return correlations roughly 0.5–0.8, scored over the *same* five
   calendar windows. Those are not 25 independent trials. Under the honest
   reading, **the +1.18 headline is not distinguishable from zero.**
3. **Even these intervals are optimistic.** The formula assumes IID normal
   returns. Real returns are autocorrelated, negatively skewed and
   volatility-clustered, all of which widen the true interval. And each fold's
   test Sharpe comes from a parameter set chosen by arg-max over the training
   grid, which biases the estimate upward by an amount this table doesn't
   attempt to capture.

**What this document still legitimately supports.** The *ranking* and the
*eliminations* survive, because they are comparative and directional rather than
absolute: MACD's catastrophic NVDA tail and EMA's dominance by WMA are visible
well outside the noise, and "don't trade this" is a much cheaper claim to justify
than "this earns 1.18." What does *not* survive is any statement of the form
"strategy X achieves Sharpe Y," and in particular any comparison of test against
train at fold level — see the note under RSI.

**What replaces this.** Horizon 5 item A in [ROADMAP.md](ROADMAP.md): stationary-bootstrap
confidence intervals on every metric, Deflated Sharpe on the parameter search,
probability of backtest overfitting via CSCV, and combinatorial purged CV to
generate enough test paths to estimate any of it. This document gets re-run and
rewritten then. The conclusions may well get weaker; that is the point of
measuring.

---

## Summary table

| Strategy        | Avg test Sharpe | Assets positive | Positive folds | Verdict       |
|-----------------|-----------------|-----------------|----------------|---------------|
| RSI             | **+1.18**       | **5 / 5**       | **20 / 25**    | ✅ TRADING    |
| Bollinger Bands | **+1.13**       | **5 / 5**       | 12 / 25        | ✅ TRADING*   |
| WMA Crossover   | +0.51           | 3 / 5           | 11 / 25        | ⚠️ AAPL only  |
| SMA Crossover   | +0.39           | 3 / 5           | 11 / 25        | ⚠️ SPY/AAPL   |
| EMA Crossover   | +0.11           | 2 / 5           | 9 / 25         | ❌ ELIMINATED |
| MACD            | **−0.28**       | 3 / 5           | 12 / 25        | ❌ ELIMINATED |

\* Bollinger Bands has many "no-signal" folds (see below).

---

## Per-strategy findings

### 1. RSI — ✅ PRIMARY TRADING CANDIDATE

**Result: positive average test Sharpe on all 5 assets.**

| Asset | Avg train Sh | Avg test Sh | Pos folds | Avg test return |
|-------|-------------|-------------|-----------|----------------|
| AAPL  | 0.74        | 0.75        | 4 / 5     | +3.7 %         |
| MSFT  | 1.29        | **1.76**    | 5 / 5     | +5.5 %         |
| GOOG  | 1.04        | 0.97        | 3 / 5     | +3.8 %         |
| NVDA  | 2.01        | **1.57**    | 4 / 5     | +8.5 %         |
| SPY   | 1.19        | 0.83        | 4 / 5     | +2.1 %         |

**The most defensible signal here**: RSI's edge is not parameter-sensitive in the
same way a crossover is. The optimizer selected periods between 10 and 28 across
folds, and the strategy stayed positive regardless. Insensitivity to the tuned
parameter is a *structural* property, and it is the one claim in this document
that a small sample does not immediately destroy — it is a statement about
consistency of sign across a parameter range, not about the size of a return.

> **Do not read test > train as strength.** MSFT (train 1.29 → test 1.76) and
> NVDA in-sample/out-of-sample inversions were originally written up here as
> evidence of robustness. They are not. Out-of-sample exceeding in-sample means
> the two estimates differ by less than their own error bars — an SE near ±1.8
> per fold swamps a gap of 0.5 — so the comparison carries essentially no
> information. If anything, a *large* inversion in either direction is evidence
> the folds are too short to compare, which is exactly the case here.

**The one consistent failure**: fold 4 (2024-09-12 → 2024-12-30) is negative on
AAPL (−1.48), NVDA (−0.02), and SPY (−1.26). All three fail in the same calendar
period — Q4 2024 was a sustained directional rally where a contrarian mean-reversion
signal is structurally disadvantaged. Awareness: RSI underperforms in strong
trending markets.

**Recommended starting params for paper trading**: period 14–22 (the modal
winner across winning folds). Start with MSFT and NVDA (cleanest edges).

---

### 2. Bollinger Bands — ✅ SECONDARY TRADING CANDIDATE

**Result: positive average test Sharpe on all 5 assets, but fold-level consistency is weaker.**

| Asset | Avg train Sh | Avg test Sh | Pos folds | Avg test return |
|-------|-------------|-------------|-----------|----------------|
| AAPL  | 1.17        | 0.97        | 2 / 5     | +1.9 %         |
| MSFT  | 1.66        | 1.28        | 3 / 5     | +3.4 %         |
| GOOG  | 1.33        | 0.52        | 2 / 5     | +1.7 %         |
| NVDA  | 1.86        | 1.11        | 2 / 5     | +5.3 %         |
| SPY   | 1.78        | **1.78**    | 3 / 5     | +4.9 %         |

**Important caveat — "no signal" folds**: many folds with 0.0 return/Sharpe are not
losses; the strategy simply produced no trades in the ~75-bar test window. Bollinger
Bands is selective by design (only fires on band-touch), so silence in a short window is
normal. The folds that *do* trade show strong results (SPY fold 0: 4.34 test Sharpe;
NVDA fold 3: 4.15; AAPL fold 3: 4.36).

**What this means in live trading**: expect periods of inactivity. Size for the long run,
not the last three months.

**Recommended starting params**: period 20, std_dev_mult 1.5–2.0. SPY is the
strongest asset for this strategy — tight markets with clear-band touches.

---

### 3. WMA Crossover — ⚠️ AAPL-SPECIFIC ONLY

Strong on AAPL (test Sharpe 1.85), moderate on MSFT and NVDA, fails on GOOG and
SPY. The asset-specificity is a concern for a diversified portfolio. Useful if
trading AAPL alone; not recommended as a general strategy.

Note that "works on one of five assets" is also what a strategy with no edge
looks like when five assets are tested — with per-fold error bars this wide, one
standout out of five is the expected amount of luck, not a finding. Treat the
AAPL result as unproven rather than asset-specific.

**Eliminated EMA**: WMA dominates EMA on every asset it covers. EMA adds no value.

---

### 4. SMA Crossover — ⚠️ CONDITIONAL

Solid on AAPL and SPY (positive test folds), but 0/5 folds positive on NVDA and
4/5 folds negative on GOOG. The SMA family is too asset-selective and superseded by
WMA where it works.

---

### 5. MACD — ❌ ELIMINATED

**Catastrophic tail: NVDA test Sharpe −2.42.** In fold 4, MACD on NVDA generated
−8.2% in the test period. The MACD parameter space (3 params: fast/slow/signal)
is larger than other strategies in this grid, which amplifies overfit — the train
optimizer picks a combo that happened to work over 175 bars, and the signal reverses
on the test slice. Not recommended for any asset until the parameter space is
significantly constrained or a longer fold is used.

---

### 6. EMA Crossover — ❌ ELIMINATED

Only 2/5 assets survive, 9/25 positive folds, avg test Sharpe +0.11. WMA
dominates it; EMA is the weakest of all three MA variants. Removed from
consideration.

---

## Trading shortlist

The strategies and assets that cleared the bar, used to drive the simulated
trading loop. Read this as "these are the ones not eliminated," not as "these
have a demonstrated edge" — per the caveat at the top, only the eliminations are
on firm statistical ground:

| Priority | Strategy        | Assets to trade | Starting params                  |
|----------|-----------------|-----------------|----------------------------------|
| 1        | RSI             | MSFT, NVDA, AAPL, GOOG, SPY | period 14–22            |
| 2        | Bollinger Bands | SPY, MSFT, NVDA              | period 20, σ 1.5–2.0    |
| 3        | WMA Crossover   | AAPL only                    | short 15, long 20–40    |

Run in parallel — the strategies are structurally different (mean-reversion vs
trend-following) so they diversify each other's failure modes.

---

## Caveats and limitations

1. **Short test windows (~75 bars / 3 months)**. A single bad trade in a short test
   window can swing Sharpe dramatically. The five-fold average is a better signal
   than any one fold — but see the statistical caveat at the top of this document
   for how much better, which is: not enough. This was the original caveat, and it
   understated the problem by treating it as a nuisance rather than as the thing
   that determines whether any number here is a result.

2. **No multiple-testing correction**. Six strategies × five assets × a parameter
   grid per fold is a large search, and the reported figures are the winners of
   that search reported at face value. Deflated Sharpe exists precisely to
   discount this and has not been applied yet.

3. **The universe is selection-biased**. Four US megacaps and their index, chosen
   in hindsight over 2020–2024 — a window containing one crash and one of the
   strongest large-cap-tech rallies on record. Mean-reversion and trend results
   from this sample should not be assumed to generalise to other regimes or to
   assets that didn't survive to be picked.

4. **Daily bars only**. These strategies use end-of-day bars with `next_open` fills.
   Intraday moves, gap opens, and overnight risk are not modelled.

5. **Q4 2024 is a known blind spot**. Multiple strategies fail in the final fold
   (2024-09-12 → 2024-12-30) across assets. This was a sustained rally period.
   Note that this "blind spot" is also just five correlated assets sharing one
   calendar window — the same non-independence that widens the error bars above.

6. **Small capital / small commission**. $10 k with $1 flat commission understates
   per-trade cost at real scale, and the results are reported at a single cost
   assumption rather than swept across a range. A strategy whose edge dies at
   15 bps is a different object from one that survives 50.

7. **Out-of-sample ≠ live**. Walk-forward removes the *parameter* overfit and
   leaves the *selection* and *sampling* problems untouched. The original text
   here named a paper soak as the next validation; that step has since been
   withdrawn (see the 2026-08-16 entry in [ROADMAP.md](ROADMAP.md)) in favour of
   bounding these numbers properly first.

---

## Next step

**Superseded (2026-08-16).** The original next step listed the Horizon 4 trading
items — dry-run adapter, live data scheduler, paper soak. The first two shipped;
the paper soak was withdrawn along with the rest of the live-trading track.

The next work on *this document* is Horizon 5 item A, which exists to replace the
numbers above with bounded ones:

1. **Stationary-bootstrap confidence intervals** on every metric, replacing the
   analytic approximation used in the caveat block with resampling that preserves
   autocorrelation and volatility clustering.
2. **Deflated Sharpe Ratio** applied to each fold's parameter search, so the
   arg-max bias is discounted rather than ignored.
3. **Probability of backtest overfitting** (CSCV) for the research process as a
   whole — one number that answers "how often does my in-sample winner
   underperform out-of-sample?"
4. **Combinatorial purged cross-validation** to replace 5 rolling folds with
   enough test paths to estimate any of the above, and to break the
   one-shared-calendar-window dependence that is currently the binding
   constraint on significance.
5. **Cost sensitivity** — report Sharpe as a surface over slippage and commission
   instead of at one point.
6. **Re-run and rewrite this document.** Expect weaker conclusions. Some
   strategies currently listed as cleared may not survive; publishing that is the
   deliverable, not a setback.
