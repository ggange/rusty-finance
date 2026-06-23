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

**Result: positive test Sharpe on all 5 assets. In two cases test > train.**

| Asset | Avg train Sh | Avg test Sh | Pos folds | Avg test return |
|-------|-------------|-------------|-----------|----------------|
| AAPL  | 0.74        | 0.75        | 4 / 5     | +3.7 %         |
| MSFT  | 1.29        | **1.76**    | 5 / 5     | +5.5 %         |
| GOOG  | 1.04        | 0.97        | 3 / 5     | +3.8 %         |
| NVDA  | 2.01        | **1.57**    | 4 / 5     | +8.5 %         |
| SPY   | 1.19        | 0.83        | 4 / 5     | +2.1 %         |

**Why this is compelling**: RSI's mean-reversion signal is not parameter-sensitive in the
same way a crossover is. The optimizer selected periods between 10 and 28 across
folds, and the strategy stayed positive regardless — a sign of genuine structural
edge rather than curve-fitting to one lucky window.

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

Strong on AAPL (test Sharpe 1.85, test > train), moderate on MSFT and NVDA,
fails on GOOG and SPY. The asset-specificity is a concern for a diversified
portfolio. Useful if trading AAPL alone; not recommended as a general strategy.

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

For the Horizon 4 paper-trading loop, use only strategies and assets that cleared
the bar:

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
   than any one fold.

2. **Daily bars only**. These strategies use end-of-day bars with `next_open` fills.
   Intraday moves, gap opens, and overnight risk are not modelled.

3. **Q4 2024 is a known blind spot**. Multiple strategies fail in the final fold
   (2024-09-12 → 2024-12-30) across assets. This was a sustained rally period.
   Paper trading through a similar regime will reveal whether the backtest predictions
   hold.

4. **Small capital / small commission**. $10 k with $1 flat commission understates
   per-trade cost at real scale. Revisit sizing and cost assumptions before live.

5. **Out-of-sample ≠ live**. This is the last backtesting check before paper. The
   next validation is the paper soak described in the ROADMAP (Horizon 4): run these
   signals through a real paper broker and compare fills against what the engine
   predicted.

---

## Next step

The tooling and the shortlist exist. The next work is entirely in Horizon 4:

1. **Broker dry-run adapter** — compute and log intended orders without sending them
   (validates the data→signal→order-intent→persistence loop at zero risk).
2. **Live data scheduler** — refresh bars daily, run RSI/BB on the latest bar.
3. **Paper soak** — run the shortlist on a paper broker for weeks; watch predicted
   vs actual fills diverge (or agree). That divergence is the deepest correctness
   test the engine can get.
