# Reading List

The theory behind rusty-finance, ordered so each item makes the next one easier.
Every section says which part of the codebase it explains, so you can read with
something concrete in front of you.

Priority marks: **★** = read this one first in its section.

## Coverage map

Which section explains which part of the platform. Use this to read against a
file rather than in the abstract — and to notice when new code arrives with no
theory behind it.

| Platform component | Section |
|---|---|
| The premise that any of this can work | §1 Market efficiency |
| `metrics.rs`, annualisation, return series | §2 Foundations |
| `data.rs`, `datasets.py`, `scripts/fetch_data.py` | §3 Data |
| `metrics.rs`, `risk.rs`, the metrics + risk panels | §4 Performance measurement |
| `sweep.rs`, `walkforward.rs`, both those tabs | §5 Backtesting & overfitting |
| `strategy/` — RSI, MACD, Bollinger, crossovers | §6 Technical trading rules |
| `SizingRule` in `portfolio.rs`, `cash_allocation` | §7 Position sizing † |
| `optimize.rs`, the weight-policy panel | §8 Portfolio construction |
| `RebalanceConfig` — monthly / quarterly / threshold | §9 Rebalancing |
| `project_to_simplex()`, gradient descent, line search | §10 Optimisation mechanics |
| `ExecutionCosts`, `FillTiming`, `engine.rs`, Soak panel | §11 Execution & microstructure |
| `trading.py`, `broker.py`, `scheduler.py`, reconcile, kill switch | §12 Live trading operations |
| The whole job, for breadth | §13 Practitioner books |
| Rust, PyO3, FastAPI, the React frontend, the test suite | §14 Implementation stack |

† §7 covers a capability the code *has* but doesn't expose — see the section.

---

## 0. If you only read four things

1. **Bailey, Borwein, López de Prado & Zhu — "Pseudo-Mathematics and Financial
   Charlatanism: The Effects of Backtest Overfitting on Out-of-Sample
   Performance"** (*Notices of the AMS*, 61(5), 2014). Short, free, and it will
   permanently change how you read the Sweep tab. Shows that with enough trials
   you can manufacture an impressive backtest from pure noise, and gives the
   minimum backtest length needed before a Sharpe ratio means anything.
2. **DeMiguel, Garlappi & Uppal — "Optimal Versus Naive Diversification: How
   Inefficient Is the 1/N Portfolio Strategy?"** (*Review of Financial Studies*,
   22(5), 2009). Fourteen optimisation models tested against equal weighting;
   none reliably beat it out-of-sample. This is the paper your `max_sharpe`
   benchmark result independently reproduced.
3. **Ernest Chan — *Quantitative Trading: How to Build Your Own Algorithmic
   Trading Business*** (Wiley, 2nd ed. 2021). The most practical on-ramp there
   is. Written by someone who ran the thing, not just modelled it.
4. **Marcos López de Prado — *Advances in Financial Machine Learning*** (Wiley,
   2018), chapters 7, 10 and 11 specifically. Ch. 7 is why standard
   cross-validation is invalid on time series; ch. 10 is bet sizing; ch. 11 is a
   taxonomy of backtest failures. You can skip the ML chapters entirely and
   still get the value.

---

## 1. Market efficiency — why should any of this work?

*Explains: the premise underneath the entire platform.*

Worth confronting before you spend months on it. If markets were perfectly
efficient, every strategy in `strategy/` would have expected excess return of
exactly zero minus costs, and the most sophisticated thing you could build would
still lose to buy-and-hold. The literature's answer is subtler than yes or no,
and it tells you *where* to look.

- ★ **Grossman & Stiglitz — "On the Impossibility of Informationally Efficient
  Markets"** (*American Economic Review*, 70(3), 1980). The single most useful
  paper here: if prices already reflected all information, nobody would be paid
  to gather it, so nobody would — therefore markets *cannot* be fully efficient.
  There must be exactly enough inefficiency left to compensate the people
  removing it. That's the space you're trying to operate in, and it also tells
  you the margin is thin by construction.
- **Fama — "Efficient Capital Markets: A Review of Theory and Empirical Work"**
  (*Journal of Finance*, 25(2), 1970). The canonical statement of the other
  side. Weak-form efficiency is precisely the claim that the rules in
  `strategy/` cannot work, since they use only past prices. Know the strongest
  version of the argument against you.
- ★ **Lo — *Adaptive Markets: Financial Evolution at the Speed of Thought***
  (Princeton, 2017), or the short version, **"The Adaptive Markets
  Hypothesis"** (*Journal of Portfolio Management*, 30(5), 2004). Efficiency as
  a variable rather than a binary: opportunities appear, get competed away, and
  reappear when conditions shift. This is the most honest framework for a
  platform like yours, and it's also the argument for why the Soak panel matters
  long-term — edges decay.
- **De Bondt & Thaler — "Does the Stock Market Overreact?"** (*Journal of
  Finance*, 40(3), 1985) and **Jegadeesh & Titman — "Returns to Buying Winners
  and Selling Losers"** (*Journal of Finance*, 48(1), 1993). Read as a pair.
  The first is the evidence for mean reversion, which is what RSI and Bollinger
  bet on; the second is the evidence for momentum, which is what the three
  crossovers and MACD bet on. Your strategy list contains both bets, in
  opposite directions, which is worth being conscious of.
- **Barberis & Thaler — "A Survey of Behavioral Finance"** (in *Handbook of the
  Economics of Finance*, 2003). Where the inefficiencies plausibly come from, if
  they exist.

---

## 2. Foundations — how returns actually behave

*Explains: `metrics.rs`, the ×252 and ×√252 annualisation throughout the code.*

- ★ **Campbell, Lo & MacKinlay — *The Econometrics of Financial Markets***
  (Princeton, 1997). The canonical graduate text. Chapters 1–2 on return
  predictability are the relevant part; the rest is reference.
- ★ **Cont — "Empirical Properties of Asset Returns: Stylized Facts and
  Statistical Issues"** (*Quantitative Finance*, 1(2), 2001). Eleven pages, free,
  and it tells you exactly why returns are not normal: fat tails, volatility
  clustering, and absence of autocorrelation in returns but strong
  autocorrelation in *absolute* returns. Read this before you trust any
  annualised-σ number — the √252 scaling in `metrics.rs` assumes i.i.d. returns,
  and this paper is the catalogue of ways that assumption fails.
- **Tsay — *Analysis of Financial Time Series*** (Wiley, 3rd ed. 2010). Where to
  go when you want GARCH and want it properly. Volatility clustering means your
  rolling-21-bar volatility is a crude version of something with real theory
  behind it.
- **Mandelbrot & Hudson — *The (Mis)Behavior of Markets*** (Basic Books, 2004).
  Popular-level, but the argument that variance understates tail risk is worth
  absorbing early.

---

## 3. Data — the layer that silently invalidates everything above it

*Explains: `data.rs`, `api/datasets.py`, `scripts/fetch_data.py`.*

The most under-appreciated section in this document. A subtle data defect
produces a *beautiful* backtest, and nothing downstream can detect it. Your
fetch script uses `auto_adjust=True`, which is the right default and also
introduces a specific bias worth understanding.

- ★ **Brown, Goetzmann, Ibbotson & Ross — "Survivorship Bias in Performance
  Studies"** (*Review of Financial Studies*, 5(4), 1992). Testing on the
  companies that exist *today* means testing on the ones that didn't go
  bankrupt. Your `data/datasets/` is AAPL, MSFT, GOOG, NVDA, SPY — five
  survivors of a period in which they were spectacular. Any strategy will look
  good on them.
- **Elton, Gruber & Blake — "Survivorship Bias and Mutual Fund Performance"**
  (*Review of Financial Studies*, 9(4), 1996). The same effect measured on
  funds, with a magnitude attached.
- ★ **Lo & MacKinlay — "Data-Snooping Biases in Tests of Financial Asset Pricing
  Models"** (*Review of Financial Studies*, 3(3), 1990). What happens when the
  dataset you test on is the dataset that suggested the hypothesis. Closely
  related to §5 but distinct: this bias enters through your *choice of data*,
  not your choice of parameters.
- **On adjusted prices** there is no single canonical paper, but the mechanic
  matters and is easy to state: split- and dividend-adjusted history is
  retroactively rewritten whenever a corporate action occurs, so a backtest run
  today sees prices that nobody could have traded at the time. This is
  look-ahead bias entering through the data layer rather than through
  `FillTiming`. Chan (§13) and López de Prado (§5) both discuss handling it;
  the CRSP and Norgate Data documentation are the clearest practitioner
  explanations of what adjustment actually does. The defensible positions are
  unadjusted prices with explicit corporate-action handling, or adjusted prices
  with the caveat stated. Silently using adjusted prices and forgetting is the
  common failure.
- **Bailey & López de Prado (2014)**, from §0, also covers the specific
  arithmetic of how much data you need before a result means anything. Your
  2020–2024 window is roughly 1,250 bars — read that paper's minimum-track-record
  length calculation against it.

---

## 4. Performance measurement — reading your own metrics honestly

*Explains: `metrics.rs`, `risk.rs` — `sharpe_ratio`, `sortino_ratio`,
`max_drawdown`, `asset_beta`, `var_95`, `cvar_95`, `contribution_to_risk`.*

- ★ **Lo — "The Statistics of Sharpe Ratios"** (*Financial Analysts Journal*,
  58(4), 2002). The Sharpe ratio is an *estimate* with a standard error, and
  when returns are autocorrelated the usual √12 or √252 annualisation is simply
  wrong. This is the paper that tells you how wide your error bars are.
- **Sharpe — "The Sharpe Ratio"** (*Journal of Portfolio Management*, 21(1),
  1994). The author's own restatement, clarifying what the ratio does and
  doesn't claim.
- **Bailey & López de Prado — "The Deflated Sharpe Ratio: Correcting for
  Selection Bias, Backtest Overfitting and Non-Normality"** (*Journal of
  Portfolio Management*, 40(5), 2014). How to discount a Sharpe ratio by the
  number of configurations you tried to find it. Directly applicable to the
  Sweep tab.
- ★ **Artzner, Delbaen, Eber & Heath — "Coherent Measures of Risk"**
  (*Mathematical Finance*, 9(3), 1999). The axiomatic case for why VaR is not a
  coherent risk measure (it can penalise diversification) and CVaR is. This is
  why the risk panel reports both and why you should read the CVaR.
- **Rockafellar & Uryasev — "Optimization of Conditional Value-at-Risk"**
  (*Journal of Risk*, 2(3), 2000). Turns CVaR into something you can actually
  optimise with linear programming — the natural next objective to add to
  `optimize.rs`.
- **Sortino & van der Meer — "Downside Risk"** (*Journal of Portfolio
  Management*, 17(4), 1991). The origin of the ratio your metrics panel reports
  next to Sharpe, and the argument for why upside deviation shouldn't be
  penalised.
- **Magdon-Ismail & Atiya — "Maximum Drawdown"** (*Risk*, 17(10), 2004). The
  expected maximum drawdown of a random walk, in closed form. Extremely useful:
  it tells you how much of your `max_drawdown` is signal and how much is just
  what a driftless process does over that many bars.

**On beta**, which `risk.rs` computes per asset against the portfolio series,
and which the SPY overlay implicitly invokes:

- ★ **Sharpe — "Capital Asset Prices: A Theory of Market Equilibrium under
  Conditions of Risk"** (*Journal of Finance*, 19(3), 1964). Where beta comes
  from and what it claims. The key idea for you: return that's merely beta
  exposure isn't skill, it's leverage on the index, and your benchmark overlay
  exists to separate the two.
- **Fama & French — "Common Risk Factors in the Returns on Stocks and Bonds"**
  (*Journal of Financial Economics*, 33(1), 1993). Beta isn't enough; size and
  value explain more. The generalisation of "is my return actually just a known
  exposure in disguise?"
- **Frazzini & Pedersen — "Betting Against Beta"** (*Journal of Financial
  Economics*, 111(1), 2014). High-beta assets underperform on a risk-adjusted
  basis. Relevant when your optimiser's weights are quietly a beta bet.

---

## 5. Backtesting and overfitting — the discipline that makes the rest worth doing

*Explains: `sweep.rs`, `walkforward.rs`, `docs/strategy-validation.md`, and why
the Sweep tab is a diagnostic rather than a selector.*

This is the section that separates a platform from a random-number generator.
If you're short on time, spend it here.

- ★ **López de Prado — *Advances in Financial Machine Learning*** (Wiley, 2018),
  **ch. 7** (cross-validation in finance — why k-fold leaks, purging, embargo)
  and **ch. 11** ("The Dangers of Backtesting"). Also **ch. 12** on the
  combinatorially-purged approach, which is the sophisticated cousin of your
  walk-forward.
- ★ **Bailey, Borwein, López de Prado & Zhu — "The Probability of Backtest
  Overfitting"** (*Journal of Computational Finance*, 20(4), 2016). Formalises
  PBO: given how many variants you tested, what's the chance your best one is
  below-median out-of-sample? Often above 50%.
- **Harvey & Liu — "Backtesting"** (*Journal of Portfolio Management*, 42(1),
  2015). Practical, readable haircuts to apply to a reported Sharpe based on
  multiple testing.
- **Harvey, Liu & Zhu — "…and the Cross-Section of Expected Returns"**
  (*Review of Financial Studies*, 29(1), 2016). Argues that after multiple-
  testing correction, a *t*-statistic of 2.0 is nowhere near enough — you need
  around 3.0. Sobering and important.
- **White — "A Reality Check for Data Snooping"** (*Econometrica*, 68(5), 2000)
  and **Sullivan, Timmermann & White — "Data-Snooping, Technical Trading Rule
  Performance, and the Bootstrap"** (*Journal of Finance*, 54(5), 1999). The
  bootstrap machinery for testing "is my best rule better than the best rule you'd
  expect from luck across this many rules?" The second paper applies it to
  exactly the kind of rules in `strategy/`.
- **Pardo — *The Evaluation and Optimization of Trading Strategies*** (Wiley,
  2nd ed. 2008). The book-length treatment of walk-forward analysis, including
  how to choose window counts and train fractions. Closest single reference to
  what your Walk-forward tab implements.
- **Bailey & López de Prado — "The Sharpe Ratio Efficient Frontier"**
  (*Journal of Risk*, 15(2), 2012). How long a track record must be before a
  Sharpe ratio is statistically distinguishable from zero.

---

## 6. Technical trading rules — do the six built-in strategies work?

*Explains: `strategy/` — RSI, MACD, Bollinger, and the three crossovers.*

- ★ **Brock, Lakonishok & LeBaron — "Simple Technical Trading Rules and the
  Stochastic Properties of Stock Returns"** (*Journal of Finance*, 47(5), 1992).
  The famous paper finding predictive power in moving-average rules on the Dow.
- **Sullivan, Timmermann & White (1999)**, above. The rebuttal: once you account
  for the universe of rules that were tried over the decades, most of Brock et
  al.'s significance evaporates. Read the two together — it's the cleanest
  worked example of data snooping in the literature.
- **Zakamulin — *Market Timing with Moving Averages*** (Palgrave, 2017). An
  unusually careful, non-promotional book-length study of moving-average rules.
  Concludes far more modestly than the trading-book genre does.
- **Lo, Mamaysky & Wang — "Foundations of Technical Analysis"** (*Journal of
  Finance*, 55(4), 2000). Formalises chart patterns with kernel regression and
  tests them. A model of how to take a fuzzy idea and make it falsifiable.

---

## 7. Position sizing — how much, not just when

*Explains: `SizingRule` in `portfolio.rs` (AllIn, FixedShares, FixedDollar,
FixedFraction) and `cash_allocation` on trade plan items.*

Your strategies decide *direction*; `SizingRule` decides *magnitude*. Sizing has
at least as much effect on the equity curve as signal quality does, and unlike
signal quality it has clean, settled theory behind it.

Worth knowing before you read further: `Portfolio::with_sizing()` exists at
`portfolio.rs:97` but is **never called anywhere in the repo**. Every backtest
and every live tick runs `SizingRule::AllIn` — the entire sleeve in, the entire
sleeve out — and there is no API field or UI control to change it. So this
section is currently theory for a knob you'd have to wire up. It is, in my view,
the highest-value gap in the platform: the other three variants are already
implemented and tested, and `FixedFraction` alone would let you ask questions
the platform cannot currently express.

- ★ **Kelly — "A New Interpretation of Information Rate"** (*Bell System
  Technical Journal*, 35(4), 1956). The origin: the bet size that maximises the
  long-run growth rate of capital. Short, and not really about gambling.
- ★ **Thorp — "The Kelly Criterion in Blackjack, Sports Betting, and the Stock
  Market"** (in *Handbook of Asset and Liability Management*, 2006; widely
  available as a preprint). The best practical exposition, by the person who
  used it in both domains. Covers fractional Kelly and why practitioners bet a
  half or a quarter of it — full Kelly is growth-optimal but has drawdowns
  almost nobody can actually sit through.
- **MacLean, Thorp & Ziemba (eds.) — *The Kelly Capital Growth Investment
  Criterion*** (World Scientific, 2011). The collected literature, including the
  objections.
- **Samuelson — "Why We Should Not Make Mean Log of Wealth Big Though Years to
  Act Are Long"** (*Journal of Banking & Finance*, 3(4), 1979). The most famous
  critique of Kelly, written entirely in one-syllable words as a rhetorical
  device. Read it so you hold the idea with appropriate caution.
- **López de Prado — *AFML*, ch. 10 ("Bet Sizing")**. How to convert a signal's
  strength into a position size rather than treating every signal identically —
  which is exactly what `AllIn` does today.
- **Moreira & Muir — "Volatility-Managed Portfolios"** (*Journal of Finance*,
  72(4), 2017). Scale position size inversely with recent volatility. Simple,
  well-evidenced, and would map cleanly onto a new `SizingRule` variant.
- **Vince — *The Mathematics of Money Management*** (Wiley, 1992). "Optimal f"
  and risk-of-ruin arithmetic. Treat the recommendations sceptically — the maths
  is sound, the sizing it implies is aggressive — but the risk-of-ruin framing is
  valuable.

---

## 8. Portfolio construction — the weight optimizer

*Explains: `backtesting/src/optimize.rs` and the weight-policy panel.*

- ★ **Markowitz — "Portfolio Selection"** (*Journal of Finance*, 7(1), 1952).
  Fourteen pages that created the field. Read the original; it's clearer than
  most summaries of it.
- ★ **Michaud — "The Markowitz Optimization Enigma: Is 'Optimized' Optimal?"**
  (*Financial Analysts Journal*, 45(1), 1989). Mean-variance optimisers are
  "estimation-error maximisers" — they load up on whatever assets the sample
  happened to flatter. This is the single best explanation of why your
  `max_sharpe` underperformed equal weight.
- ★ **Ledoit & Wolf — "Honey, I Shrunk the Sample Covariance Matrix"**
  (*Journal of Portfolio Management*, 30(4), 2004), and the technical version,
  **"A Well-Conditioned Estimator for Large-Dimensional Covariance Matrices"**
  (*Journal of Multivariate Analysis*, 88(2), 2004). The theory behind your
  `shrinkage` parameter — including how to choose it optimally rather than
  defaulting to 0.2.
- **Jorion — "Bayes-Stein Estimation for Portfolio Analysis"** (*Journal of
  Financial and Quantitative Analysis*, 21(3), 1986). Shrinkage applied to
  expected returns, which is the input `max_sharpe` most needs help with.
- ★ **Maillard, Roncalli & Teiletche — "The Properties of Equally Weighted Risk
  Contribution Portfolios"** (*Journal of Portfolio Management*, 36(4), 2010).
  The reference for risk parity: existence, uniqueness, and the fact that it
  sits between minimum-variance and equal-weight. Read alongside your
  `risk_contribution()`.
- **Roncalli — *Introduction to Risk Parity and Budgeting*** (Chapman & Hall,
  2013). Book-length, rigorous, and covers the numerical methods — including why
  a naive multiplicative fixed-point iteration oscillates and needs damping,
  which is precisely the problem your solver hit.
- **López de Prado — "Building Diversified Portfolios that Outperform Out of
  Sample"** (*Journal of Portfolio Management*, 42(4), 2016). Hierarchical Risk
  Parity: cluster the correlation matrix and allocate down the tree, avoiding
  matrix inversion entirely. A strong candidate for a sixth objective.
- **Clarke, de Silva & Thorley — "Minimum-Variance Portfolio Composition"**
  (*Journal of Portfolio Management*, 37(2), 2011). Why long-only minimum-
  variance portfolios end up concentrated, which is what your `max_weight` cap
  exists to control.
- **Grinold & Kahn — *Active Portfolio Management*** (McGraw-Hill, 2nd ed.
  1999). The industry standard. The Fundamental Law of Active Management —
  IR ≈ IC × √breadth — is the framework for asking whether a strategy is worth
  running at all.
- **Ang — *Asset Management: A Systematic Approach to Factor Investing***
  (Oxford, 2014). The best modern treatment of what actually drives returns, and
  a useful corrective if you start believing signal comes from indicators.

---

## 9. Rebalancing — the choice hiding behind monthly/quarterly/threshold

*Explains: `RebalanceConfig` and `RebalanceFrequency` in
`portfolio_backtest.rs`, and the rebalance dropdown in the config panel.*

Choosing "quarterly" in the UI looks like an operational detail. It isn't: a
rebalancing rule is a trading strategy in its own right, with a return
distribution of its own, and it is implicitly a bet on mean reversion.

- ★ **Perold & Sharpe — "Dynamic Strategies for Asset Allocation"** (*Financial
  Analysts Journal*, 44(1), 1988). The one to read. Compares buy-and-hold,
  constant-mix (which is what your rebalancing does) and CPPI, and shows their
  payoff diagrams are fundamentally different shapes. Constant-mix sells winners
  and buys losers, so it outperforms in oscillating markets and underperforms in
  trending ones. That is a real bet, and the UI currently lets you make it
  without noticing.
- **Ilmanen & Maloney — "Portfolio Rebalancing"** (AQR, 2015; free). The modern
  practitioner treatment: how much rebalancing is worth, and the interaction
  with momentum, which cuts the opposite way.
- **Donohue & Yip — "Optimal Portfolio Rebalancing with Transaction Costs"**
  (*Journal of Portfolio Management*, 29(4), 2003). Why threshold rebalancing
  generally dominates calendar rebalancing once costs are real — directly
  relevant to your `threshold` option and worth reading before defaulting to
  quarterly.
- **Sun, Fan, Chen, Schouwenaars & Albota — "Optimal Rebalancing for
  Institutional Portfolios"** (*Journal of Portfolio Management*, 32(2), 2006).
  Frames the no-trade region properly, which is what a drift threshold
  approximates.

---

## 10. Optimisation mechanics — how the solver works

*Explains: `project_to_simplex()`, the projected gradient descent, the
backtracking line search, `covariance()`.*

- ★ **Boyd & Vandenberghe — *Convex Optimization*** (Cambridge, 2004). **Free
  PDF from Stanford.** Chapter 9 (unconstrained methods, backtracking line
  search) and the projection material are directly what `optimize.rs` does.
  Genuinely one of the best-written technical books in any field.
- ★ **Duchi, Shalev-Shwartz, Singer & Chandra — "Efficient Projections onto the
  ℓ₁-Ball for Learning in High Dimensions"** (*ICML*, 2008). The simplex
  projection algorithm your code cites by name. Six pages.
- **Nocedal & Wright — *Numerical Optimization*** (Springer, 2nd ed. 2006). The
  reference when you need convergence guarantees rather than intuition.
- **Bailey & López de Prado — "An Open-Source Implementation of the Critical-Line
  Algorithm for Portfolio Optimization"** (*Algorithms*, 6(1), 2013). Markowitz's
  own exact method for the constrained efficient frontier — the alternative to
  iterating.
- **Higham — *Accuracy and Stability of Numerical Algorithms*** (SIAM, 2nd ed.
  2002). Covariance estimation is a textbook source of catastrophic
  cancellation, and your `covariance()` runs on f64 over thousands of small
  returns. This is the reference for knowing when that matters.

---

## 11. Execution, costs and market microstructure

*Explains: `ExecutionCosts`, `FillTiming` in `engine.rs`, `commission`,
`slippage_pct`, and the Soak panel.*

- ★ **Almgren & Chriss — "Optimal Execution of Portfolio Transactions"**
  (*Journal of Risk*, 3(2), 2000). The foundational trade-off between market
  impact (trade fast, pay more) and volatility risk (trade slow, risk more).
- ★ **Perold — "The Implementation Shortfall: Paper Versus Reality"** (*Journal
  of Portfolio Management*, 14(3), 1988). Names and frames the exact gap your
  soak report measures — the difference between the paper portfolio's price and
  what you actually got.
- **Kissell — *The Science of Algorithmic Trading and Portfolio Management***
  (Academic Press, 2013). Practical transaction-cost analysis: how to measure
  implementation shortfall, which is conceptually what your soak's
  slippage-in-bps number is.
- **Johnson — *Algorithmic Trading and DMA*** (4Myeloma Press, 2010). Order
  types, venues, and what actually happens after you press send. Unglamorous and
  extremely useful before connecting a real broker.
- **Harris — *Trading and Exchanges: Market Microstructure for
  Practitioners*** (Oxford, 2003). Why the bid-ask spread exists and who's on
  the other side of your fill. The best answer to "why did my backtest's edge
  disappear in production."
- **Roll — "A Simple Implicit Measure of the Effective Bid-Ask Spread in an
  Efficient Market"** (*Journal of Finance*, 39(4), 1984). Estimate the spread
  from daily closes alone — useful because that's all your OHLCV data has, and
  it would give you a defensible `slippage_pct` instead of a guess.

---

## 12. Live trading operations — the part that isn't finance

*Explains: `trading.py` (the position ledger, tick idempotency, reconciliation),
`broker.py`, `scheduler.py`, risk limits, and the kill switch.*

Everything in the Trading tab is a distributed-systems problem wearing a
finance costume: you have two systems of record (your ledger and the venue's)
that must agree, an operation that must not double-execute, and a scheduled job
that runs unattended with real consequences.

- ★ **Kleppmann — *Designing Data-Intensive Applications*** (O'Reilly, 2017).
  Chapters 7–9 on transactions, distributed trouble, and consistency. Your tick
  idempotency ("no duplicate BUY while long") and your reconcile endpoint are
  textbook instances of problems this book names precisely. If you only read one
  engineering book for the trading track, read this one.
- ★ **Nygard — *Release It!*** (Pragmatic Bookshelf, 2nd ed. 2018). Circuit
  breakers, bulkheads, timeouts, and the stability antipatterns. Your kill
  switch is a circuit breaker and your `max_daily_orders` is a bulkhead; this
  book names both and will suggest the ones you're missing.
- **Beyer, Jones, Petoff & Murphy (eds.) — *Site Reliability Engineering***
  (O'Reilly, 2016; **free online**). The chapters on monitoring, alerting and
  postmortems. A scheduler firing at 16:30 unattended needs the discipline this
  book describes, and "what should page a human?" is a question your Status
  Strip only half-answers.
- **Fowler — "Event Sourcing" and "Accounting Patterns"** (martinfowler.com;
  free). Your position ledger is double-entry accounting with a different
  vocabulary. These short articles explain why an append-only record of
  *intents* plus a derived position table is the right shape — which is
  incidentally what you already built.
- **Page — "Continuous Inspection Schemes"** (*Biometrika*, 41(1/2), 1954). The
  CUSUM control chart. This is the statistically principled way to answer "has
  my live fill quality or PnL drifted from what the backtest predicted?" — the
  question the Soak panel poses but doesn't yet test. López de Prado's *AFML*
  ch. 2 uses CUSUM filters in a related way.
- **Chan — *Algorithmic Trading*** (§13), ch. 8, on the practical gap between a
  working backtest and a working live system.

---

## 13. Practitioner books — the shape of the whole job

- ★ **Chan — *Algorithmic Trading: Winning Strategies and Their Rationale***
  (Wiley, 2013). The sequel to the one in §0, and better. Mean reversion vs
  momentum, with the reasoning for why each should work.
- **Narang — *Inside the Black Box: A Simple Guide to Quantitative and High-
  Frequency Trading*** (Wiley, 2nd ed. 2013). What a real quant shop's stack
  looks like end to end — you'll recognise your own architecture in it.
- **Ilmanen — *Expected Returns*** (Wiley, 2011). Encyclopaedic on where returns
  actually come from across every asset class. The antidote to indicator-hunting.
- **Taleb — *Fooled by Randomness*** (Random House, 2001). Read it after your
  first strategy that looks amazing in-sample.
- **Bandy — *Quantitative Technical Analysis*** (Blue Owl, 2015). Unusually
  statistically literate for the retail genre; strong on position sizing and on
  deciding when a live strategy has stopped working.

---

## 14. The implementation stack

*Explains: how the thing you built is built.*

**Rust and the bridge**

- **Blandy, Orendorff & Tindall — *Programming Rust*** (O'Reilly, 2nd ed. 2021).
  The best Rust book for people who already program.
- **Gjengset — *Rust for Rustaceans*** (No Starch, 2021). The next step: what
  the ownership rules mean for API design.
- **The PyO3 user guide** (pyo3.rs) and **maturin docs** (maturin.rs). Your
  `backtesting-py` bridge. The chapters on error conversion and the GIL are the
  ones that matter if you ever want to release the GIL and parallelise the sweep.
- **The rayon docs** (docs.rs/rayon). The sweep is embarrassingly parallel and
  currently isn't — this is the shortest path from "Rust is fast" to "the sweep
  is 8× faster."

**The API layer**

- **The FastAPI docs** (fastapi.tiangolo.com), especially discriminated unions in
  Pydantic v2 — that's the mechanism behind `WeightPolicyIn` and
  `StrategyParams`, and the thing keeping your Rust serde tagged enums and your
  HTTP schema in agreement.

**Testing numerical code**

Worth a mention given the repo runs 400+ tests across three languages.

- **Goldberg — "What Every Computer Scientist Should Know About Floating-Point
  Arithmetic"** (*ACM Computing Surveys*, 23(1), 1991; free). Why your tests
  compare with epsilons and not `==`. Essential for a codebase that is almost
  entirely f64.
- **Claessen & Hughes — "QuickCheck: A Lightweight Tool for Random Testing of
  Haskell Programs"** (*ICFP*, 2000). The origin of property-based testing. The
  natural fit for `optimize.rs`: weights always sum to 1, are never negative,
  and never exceed the cap — properties that should hold for *any* input, which
  is much stronger than the specific cases you've pinned. The `proptest` crate
  brings this to Rust.

**The frontend**

- **Wilke — *Fundamentals of Data Visualization*** (O'Reilly, 2019; **free
  online**). The most practical modern reference, and directly applicable to the
  equity, drawdown and rolling-volatility charts.
- **Tufte — *The Visual Display of Quantitative Information*** (Graphics Press,
  2nd ed. 2001). Data-ink ratio and chartjunk. The classic.
- **Few — *Information Dashboard Design*** (O'Reilly, 2nd ed. 2013). Your
  Trading tab *is* a dashboard, with the specific job of making "is this thing
  OK?" answerable at a glance. This book is about exactly that problem.
- **Cleveland & McGill — "Graphical Perception: Theory, Experimentation, and
  Application to the Development of Graphical Methods"** (*JASA*, 79(387),
  1984). The empirical ranking of which visual encodings people read accurately.
  Explains why your correlation matrix is harder to read than your line charts.

---

## 15. Ongoing sources

- **arXiv q-fin.PM and q-fin.TR** — where most of the above appears first, free.
- **SSRN**, Financial Economics Network — same, plus practitioner work.
- **Quantitative Finance**, **Journal of Portfolio Management**, **Financial
  Analysts Journal** — the three journals that carry most of §8.
- **AQR's research library** (aqr.com) — practitioner papers, free, unusually
  candid about what doesn't work.
- **Quantopian's lecture series** (the company is gone, the notebooks are
  archived on GitHub) — free, well-made, and pitched at exactly this level.
- **QuantEcon** (quantecon.org) — excellent free lectures on the numerical and
  economic-modelling side.

---

## A suggested order

If you want a path rather than a menu:

**Week 1 — is there anything here at all?** §1, in particular Grossman-Stiglitz
and Lo. Frames every result you'll get afterwards.

**Weeks 2–3 — don't get fooled.** §0 items 1 and 4, then Cont (§2), Lo's Sharpe
paper (§4), and Brown et al. on survivorship (§3). You'll never read a Sharpe
ratio the same way again — and you'll look at your five-survivor dataset
differently.

**Weeks 4–5 — validate properly.** §5 in full, then Brock et al. against
Sullivan et al. (§6). Then re-run your own Walk-forward tab and re-read
`strategy-validation.md` with new eyes.

**Week 6 — how much.** §7. Short section, large effect, and the one most likely
to change your equity curve immediately.

**Weeks 7–9 — construct portfolios.** Markowitz → Michaud → Ledoit-Wolf →
Maillard et al. (§8), reading `optimize.rs` alongside, then Perold & Sharpe
(§9). Boyd ch. 9 and the Duchi paper (§10) when you want to know *how* rather
than *what*.

**Weeks 10–11 — face reality.** §11, then compare Almgren-Chriss's impact model
to what your soak report is actually measuring.

**Week 12 — run it safely.** §12, mainly Kleppmann and Nygard. Do this before,
not after, connecting anything with real money.

**Ongoing.** §13 for breadth, §15 for what's new.

The order is deliberate: scepticism before construction, and construction before
operation. It's much easier to build a portfolio optimiser than to know whether
the one you built helped, and this platform now gives you the tools to answer
the second question — the literature above is how you learn to ask it properly.
