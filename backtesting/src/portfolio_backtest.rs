//! Multi-asset portfolio backtesting.
//!
//! Runs each asset through the single-asset [`BacktestEngine`] with its own
//! slice of capital and its own strategy, then aggregates the per-asset equity
//! curves into one portfolio-level curve, metrics, and benchmark. The
//! single-asset engine, [`Portfolio`], and [`Metrics`] are reused unchanged.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::data::Candle;
use crate::engine::{BacktestEngine, BacktestResult, FillTiming};
use crate::metrics::{Benchmark, Metrics};
use crate::optimize::{optimize_weights, OptimizerConfig};
use crate::portfolio::{EquityPoint, ExecutionCosts, Portfolio, TradeRecord};
use crate::risk::RiskMetrics;
use crate::strategy::Strategy;

/// How often the portfolio is rebalanced back to target weights.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RebalanceFrequency {
    /// Rebalance on the first trading day of each calendar month.
    Monthly,
    /// Rebalance on the first trading day of each calendar quarter.
    Quarterly,
    /// Rebalance whenever any asset drifts more than `threshold` (e.g. 0.05 = 5 %) from its target weight.
    Threshold { threshold: f64 },
}

/// Optional rebalancing overlay for a portfolio backtest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceConfig {
    pub frequency: RebalanceFrequency,
}

/// How the portfolio's target weights are decided.
///
/// All three variants resolve to the same question the rebalancing overlay
/// already answers — *at this date, what are the target weights?* — so they
/// share one code path and differ only in where the targets come from.
///
/// ## No look-ahead, by construction
///
/// [`WeightPolicy::Static`] does **not** solve over the full history and then
/// allocate from day one; that would set the opening allocation using returns
/// that had not happened yet. Instead it spends a warm-up window estimating,
/// runs the manual weights meanwhile, and switches to the solved weights once
/// the window is full. [`WeightPolicy::Dynamic`] re-solves at each rebalance
/// date from a trailing window. Neither ever reads a return from the future.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WeightPolicy {
    /// Use the weights supplied on each [`PortfolioAsset`].
    Manual,
    /// Solve once, after `warmup` observations, then hold those weights. Any
    /// configured rebalance schedule maintains them thereafter.
    Static {
        optimizer: OptimizerConfig,
        /// Observations to accumulate before solving. Roughly one trading year
        /// (252) is a reasonable floor for a covariance estimate.
        warmup: usize,
    },
    /// Re-solve at every rebalance date from the trailing `lookback` window.
    ///
    /// Solving does not begin until a full window is available; the manual
    /// weights are used until then. This is what the Rust engine buys you: a
    /// solve per rebalance date over the whole history is hundreds of
    /// optimizations, and it stays cheap.
    Dynamic {
        optimizer: OptimizerConfig,
        /// Trailing observations each solve sees.
        lookback: usize,
    },
}

impl Default for WeightPolicy {
    fn default() -> Self {
        WeightPolicy::Manual
    }
}

/// The target weights in force from a given date, and why.
#[derive(Debug, Clone, Serialize)]
pub struct WeightSnapshot {
    pub date: NaiveDate,
    pub weights: Vec<f64>,
    /// Annualized volatility the optimizer expected from these weights, when
    /// they came from a solve. `None` for manual weights.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_volatility: Option<f64>,
    /// Each asset's share of predicted portfolio risk, when solved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_contribution: Option<Vec<f64>>,
}

/// One asset to include in a portfolio backtest.
pub struct PortfolioAsset {
    /// Ticker/label, e.g. `"AAPL"`.
    pub symbol: String,
    /// Relative capital weight. Normalized across all assets so they sum to 1.
    pub weight: f64,
    /// Price history for this asset, in chronological order.
    pub candles: Vec<Candle>,
    /// The (boxed) strategy applied to this asset.
    pub strategy: Box<dyn Strategy>,
}

/// The result for a single asset within a portfolio run: the full single-asset
/// [`BacktestResult`] plus the capital it was allocated.
#[derive(Debug, Serialize, Clone)]
pub struct AssetResult {
    pub symbol: String,
    pub weight: f64,
    pub allocated_cash: f64,
    #[serde(flatten)]
    pub result: BacktestResult,
}

/// The aggregate output of a portfolio backtest.
#[derive(Debug, Serialize, Clone)]
pub struct PortfolioResult {
    /// Portfolio NAV over the union of all asset dates.
    pub equity_curve: Vec<EquityPoint>,
    /// Portfolio-level performance statistics.
    pub metrics: Metrics,
    /// Weighted buy-and-hold benchmark across all assets.
    pub benchmark: Benchmark,
    /// Risk analytics: correlation, VaR/CVaR, rolling vol, contribution to risk.
    pub risk: RiskMetrics,
    /// Per-asset breakdown.
    pub assets: Vec<AssetResult>,
    /// Dates on which the portfolio was rebalanced (empty if no rebalancing configured).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rebalance_dates: Vec<NaiveDate>,
    /// Every change of target weights, in order. Empty under
    /// [`WeightPolicy::Manual`], one entry for [`WeightPolicy::Static`], and one
    /// per re-solve for [`WeightPolicy::Dynamic`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub weight_history: Vec<WeightSnapshot>,
}

/// Normalize weights so they sum to 1. If every weight is zero (or negative
/// sum), fall back to equal weighting.
fn normalize_weights(weights: &[f64]) -> Vec<f64> {
    let n = weights.len();
    if n == 0 {
        return Vec::new();
    }
    let sum: f64 = weights.iter().map(|w| w.max(0.0)).sum();
    if sum <= 0.0 {
        return vec![1.0 / n as f64; n];
    }
    weights.iter().map(|w| w.max(0.0) / sum).collect()
}

/// Run a multi-asset backtest. Capital is split across assets by normalized
/// weight; each asset runs independently through the single-asset engine, and
/// the per-asset NAV curves are summed (by date) into the portfolio curve.
///
/// If `rebalance` is `Some`, a NAV-overlay rebalancing is applied: on each
/// designated date the portfolio is virtually sold and rebought at target weights,
/// with execution costs deducted from the total portfolio value.
pub fn run_portfolio(
    assets: Vec<PortfolioAsset>,
    initial_cash: f64,
    costs: ExecutionCosts,
    rebalance: Option<RebalanceConfig>,
    fill_timing: FillTiming,
) -> PortfolioResult {
    run_portfolio_with_policy(
        assets,
        initial_cash,
        costs,
        rebalance,
        fill_timing,
        WeightPolicy::Manual,
    )
}

/// Run a multi-asset backtest under an explicit [`WeightPolicy`].
///
/// The engines run once, on the manual allocation. Solved weights are then
/// applied through the same NAV-overlay the rebalancer uses, which is exact
/// because an asset's *return stream* does not depend on how much capital it
/// was given — only its NAV level does.
pub fn run_portfolio_with_policy(
    assets: Vec<PortfolioAsset>,
    initial_cash: f64,
    costs: ExecutionCosts,
    rebalance: Option<RebalanceConfig>,
    fill_timing: FillTiming,
    policy: WeightPolicy,
) -> PortfolioResult {
    let weights: Vec<f64> = assets.iter().map(|a| a.weight).collect();
    let normalized = normalize_weights(&weights);

    let mut asset_results: Vec<AssetResult> = Vec::with_capacity(assets.len());
    for (asset, w) in assets.into_iter().zip(normalized.into_iter()) {
        let allocated_cash = initial_cash * w;
        let portfolio = Portfolio::new(allocated_cash, asset.symbol.clone()).with_costs(costs.clone());
        let mut engine = BacktestEngine::new(asset.strategy, portfolio).with_fill_timing(fill_timing);
        engine.run(&asset.candles);
        asset_results.push(AssetResult {
            symbol: asset.symbol,
            weight: w,
            allocated_cash,
            result: engine.result(),
        });
    }

    let agg = aggregate(&asset_results, initial_cash, rebalance.as_ref(), &costs, &policy);
    PortfolioResult {
        equity_curve: agg.equity_curve,
        metrics: agg.metrics,
        benchmark: agg.benchmark,
        risk: agg.risk,
        assets: asset_results,
        rebalance_dates: agg.rebalance_dates,
        weight_history: agg.weight_history,
    }
}

/// Everything [`aggregate`] derives from the per-asset results.
struct Aggregated {
    equity_curve: Vec<EquityPoint>,
    metrics: Metrics,
    benchmark: Benchmark,
    risk: RiskMetrics,
    rebalance_dates: Vec<NaiveDate>,
    weight_history: Vec<WeightSnapshot>,
}

/// Build the portfolio equity curve, metrics, benchmark, and risk analytics from
/// the per-asset results. Sums each asset's as-of NAV across the union of all dates
/// (forward-filling each asset's last known NAV; before an asset's first bar it
/// contributes its allocated cash).
///
/// If `rebalance` is `Some`, applies a NAV-overlay: on each rebalance date, the
/// aggregate NAV is virtually reallocated to target weights, with execution costs
/// deducted from the total.
fn aggregate(
    assets: &[AssetResult],
    initial_cash: f64,
    rebalance: Option<&RebalanceConfig>,
    costs: &ExecutionCosts,
    policy: &WeightPolicy,
) -> Aggregated {
    // Union of every date that appears in any asset's curve, sorted.
    let mut dates: BTreeMap<NaiveDate, ()> = BTreeMap::new();
    for a in assets {
        for ep in &a.result.equity_curve {
            dates.insert(ep.date, ());
        }
    }
    let dates_vec: Vec<NaiveDate> = dates.keys().copied().collect();

    // Unadjusted per-asset NAV and return series, aligned to the union of dates.
    // These feed the optimizer. Multipliers are deliberately excluded: scaling an
    // asset's capital scales its NAV but leaves its *return stream* unchanged, so
    // the raw series is the right estimation input and avoids a feedback loop
    // where past rebalances distort the next solve.
    let raw_navs: Vec<Vec<f64>> = assets
        .iter()
        .map(|a| {
            dates_vec
                .iter()
                .map(|&d| asof_nav(&a.result.equity_curve, d, a.allocated_cash))
                .collect()
        })
        .collect();
    let raw_returns: Vec<Vec<f64>> = raw_navs
        .iter()
        .map(|navs| {
            navs.windows(2)
                .map(|w| if w[0] != 0.0 { w[1] / w[0] - 1.0 } else { 0.0 })
                .collect::<Vec<f64>>()
        })
        .collect();

    let manual_weights: Vec<f64> = assets.iter().map(|a| a.weight).collect();

    // Dynamic weights only take effect at rebalance events, so a dynamic policy
    // with no schedule would silently never re-solve. Fall back to monthly.
    let effective_rebalance: Option<RebalanceConfig> = match (rebalance, policy) {
        (Some(cfg), _) => Some(cfg.clone()),
        (None, WeightPolicy::Dynamic { .. }) => {
            Some(RebalanceConfig { frequency: RebalanceFrequency::Monthly })
        }
        (None, _) => None,
    };

    // Each asset's live NAV inside the portfolio, evolved by its own return each
    // bar and reset to the target split on each rebalance.
    //
    // This tracks levels rather than accumulating multiplicative rebalance
    // ratios. The distinction matters once an optimizer is in play: a solver may
    // legitimately take a weight to ~0 and later restore it, and a ratio-based
    // multiplier would need `target / ≈0` to do that — which overflows to
    // infinity and poisons the curve with NaN. Levels re-fund an asset cleanly.
    let mut holdings: Vec<f64> = assets.iter().map(|a| a.allocated_cash).collect();
    let mut rebalance_dates: Vec<NaiveDate> = Vec::new();
    let mut weight_history: Vec<WeightSnapshot> = Vec::new();
    let mut targets: Vec<f64> = manual_weights.clone();
    let mut static_solved = false;
    let mut prev_date: Option<NaiveDate> = None;

    let mut equity_curve: Vec<EquityPoint> = Vec::with_capacity(dates_vec.len());
    for (k, &date) in dates_vec.iter().enumerate() {
        // Grow each holding by its asset's return over the bar just ended.
        if k > 0 {
            for i in 0..assets.len() {
                holdings[i] *= 1.0 + raw_returns[i][k - 1];
            }
        }
        let adj_navs = holdings.clone();
        let total: f64 = adj_navs.iter().sum();

        // `k` dates means k−1 returns are available strictly before this bar.
        let available = k.saturating_sub(1);

        // Does the schedule fire here?
        let scheduled = match (&effective_rebalance, prev_date) {
            (Some(cfg), Some(prev)) => match &cfg.frequency {
                RebalanceFrequency::Monthly => {
                    date.month() != prev.month() || date.year() != prev.year()
                }
                RebalanceFrequency::Quarterly => {
                    let q = |d: NaiveDate| (d.month() - 1) / 3;
                    q(date) != q(prev) || date.year() != prev.year()
                }
                RebalanceFrequency::Threshold { threshold } => {
                    total > 0.0
                        && (0..assets.len())
                            .any(|i| (adj_navs[i] / total - targets[i]).abs() > *threshold)
                }
            },
            _ => false,
        };

        // Does the weight policy want new targets here? A static solve forces a
        // rebalance of its own the moment its warm-up is satisfied, so the
        // solved weights take effect without waiting for the next schedule date.
        let mut solved: Option<crate::optimize::WeightSolution> = None;
        let mut policy_forces = false;
        match policy {
            WeightPolicy::Manual => {}
            WeightPolicy::Static { optimizer, warmup } => {
                if !static_solved && available >= (*warmup).max(2) {
                    solved = Some(optimize_weights(&window(&raw_returns, available, available), optimizer));
                    static_solved = true;
                    policy_forces = true;
                }
            }
            WeightPolicy::Dynamic { optimizer, lookback } => {
                // Wait for a full window, mirroring how Static waits for its
                // warm-up. Solving on a partial window produces a covariance
                // estimate from a handful of points — noise that would still
                // move real capital. Lower `lookback` to start solving sooner.
                if scheduled && available >= (*lookback).max(2) {
                    solved = Some(optimize_weights(&window(&raw_returns, available, *lookback), optimizer));
                }
            }
        }

        if let Some(sol) = &solved {
            targets = sol.weights.clone();
            weight_history.push(WeightSnapshot {
                date,
                weights: sol.weights.clone(),
                expected_volatility: Some(sol.expected_volatility),
                risk_contribution: Some(sol.risk_contribution.clone()),
            });
        }

        if (scheduled || policy_forces) && total > 0.0 {
            // Execution cost: slippage on each leg + commission per asset traded.
            let slippage_cost: f64 = (0..assets.len())
                .map(|i| (adj_navs[i] - total * targets[i]).abs() * costs.slippage_pct)
                .sum();
            let commission_cost = costs.commission_per_trade * assets.len() as f64;
            let total_after_costs = (total - slippage_cost - commission_cost).max(0.0);

            // Reset each holding to its share of the post-cost portfolio.
            for i in 0..assets.len() {
                holdings[i] = total_after_costs * targets[i];
            }
            rebalance_dates.push(date);
        }

        equity_curve.push(EquityPoint { date, nav: holdings.iter().sum() });
        prev_date = Some(date);
    }

    // Per-asset return series for risk analytics. Returns are scale-invariant,
    // so the raw series is what RiskMetrics wants — rebalance-adjusted levels
    // would inject artificial jumps at each rebalance date.
    let asset_returns: Vec<Vec<f64>> = raw_returns.clone();
    let symbols: Vec<String> = assets.iter().map(|a| a.symbol.clone()).collect();
    // Risk analytics describe the allocation actually in force at the end, which
    // under a solving policy is not the manual weight vector.
    let risk = RiskMetrics::compute(&symbols, &asset_returns, &targets, &equity_curve);

    // Concatenate all trades for portfolio-level trade stats (win_rate, count).
    let all_trades: Vec<TradeRecord> =
        assets.iter().flat_map(|a| a.result.trades.iter().cloned()).collect();
    let metrics = Metrics::compute(&equity_curve, &all_trades);

    // Weighted buy-and-hold: sum each asset's benchmark end NAV.
    let end_nav: f64 = assets
        .iter()
        .map(|a| a.allocated_cash * (1.0 + a.result.benchmark.total_return))
        .sum();
    let benchmark = if initial_cash > 0.0 && !equity_curve.is_empty() {
        let total_return = end_nav / initial_cash - 1.0;
        let n = equity_curve.len() as f64;
        let cagr = (end_nav / initial_cash).powf(252.0 / n) - 1.0;
        Benchmark { total_return, cagr }
    } else {
        Benchmark { total_return: 0.0, cagr: 0.0 }
    };

    Aggregated { equity_curve, metrics, benchmark, risk, rebalance_dates, weight_history }
}

/// The trailing `size` returns ending at index `end` (exclusive), per asset.
///
/// Slicing at `end` is what keeps every solve honest: index `end` corresponds to
/// the bar being decided, so nothing at or after it is visible.
fn window(returns: &[Vec<f64>], end: usize, size: usize) -> Vec<Vec<f64>> {
    let start = end.saturating_sub(size);
    returns
        .iter()
        .map(|r| {
            let hi = end.min(r.len());
            let lo = start.min(hi);
            r[lo..hi].to_vec()
        })
        .collect()
}

/// NAV of one asset as of `date`: the most recent equity point on or before
/// `date`, or `fallback` (its allocated cash) if it has not started trading yet.
fn asof_nav(curve: &[EquityPoint], date: NaiveDate, fallback: f64) -> f64 {
    let mut nav = fallback;
    for ep in curve {
        if ep.date <= date {
            nav = ep.nav;
        } else {
            break;
        }
    }
    nav
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::Signal;
    use std::collections::VecDeque;

    /// Replays a fixed sequence of signals, one per bar.
    struct ScriptedStrategy {
        signals: VecDeque<Signal>,
    }

    impl ScriptedStrategy {
        fn new(signals: Vec<Signal>) -> Self {
            Self { signals: signals.into() }
        }
    }

    impl Strategy for ScriptedStrategy {
        fn on_bar(&mut self, _candle: &Candle) -> Signal {
            self.signals.pop_front().unwrap_or(Signal::Hold)
        }
    }

    fn candle(close: f64, day: u32) -> Candle {
        Candle {
            date: NaiveDate::from_ymd_opt(2024, 1, day).unwrap(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 0,
        }
    }

    fn candles(closes: &[f64]) -> Vec<Candle> {
        closes.iter().enumerate().map(|(i, &c)| candle(c, i as u32 + 1)).collect()
    }

    /// Candles on consecutive calendar days, for series longer than one month.
    fn long_candles(closes: &[f64]) -> Vec<Candle> {
        let start = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| Candle {
                date: start + chrono::Duration::days(i as i64),
                open: c,
                high: c,
                low: c,
                close: c,
                volume: 0,
            })
            .collect()
    }

    /// Buy on the first bar and hold, so NAV tracks price and the asset's return
    /// stream is the price return stream.
    fn buy_and_hold(n: usize) -> Box<dyn Strategy> {
        let signals = std::iter::once(Signal::Buy)
            .chain(std::iter::repeat(Signal::Hold).take(n - 1))
            .collect();
        Box::new(ScriptedStrategy::new(signals))
    }

    /// A deterministic price path whose per-bar volatility switches partway
    /// through, so "calm early, wild late" can be constructed exactly.
    fn switching_prices(n: usize, first_vol: f64, second_vol: f64, switch_at: usize) -> Vec<f64> {
        let mut price = 100.0;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let vol = if i < switch_at { first_vol } else { second_vol };
            // Deterministic alternating shock: no drift, exact volatility.
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            price *= 1.0 + sign * vol;
            out.push(price);
        }
        out
    }

    #[test]
    fn weights_normalize_to_one() {
        let w = normalize_weights(&[1.0, 3.0]);
        assert!((w[0] - 0.25).abs() < 1e-9);
        assert!((w[1] - 0.75).abs() < 1e-9);
    }

    #[test]
    fn zero_weights_fall_back_to_equal() {
        let w = normalize_weights(&[0.0, 0.0, 0.0]);
        assert!(w.iter().all(|x| (x - 1.0 / 3.0).abs() < 1e-9));
    }

    #[test]
    fn equal_weight_splits_capital() {
        let assets = vec![
            PortfolioAsset {
                symbol: "A".into(),
                weight: 1.0,
                candles: candles(&[100.0, 100.0]),
                strategy: Box::new(ScriptedStrategy::new(vec![Signal::Hold, Signal::Hold])),
            },
            PortfolioAsset {
                symbol: "B".into(),
                weight: 1.0,
                candles: candles(&[100.0, 100.0]),
                strategy: Box::new(ScriptedStrategy::new(vec![Signal::Hold, Signal::Hold])),
            },
        ];
        let res = run_portfolio(assets, 10_000.0, ExecutionCosts::default(), None, FillTiming::Close);
        assert!((res.assets[0].allocated_cash - 5_000.0).abs() < 1e-9);
        assert!((res.assets[1].allocated_cash - 5_000.0).abs() < 1e-9);
    }

    #[test]
    fn portfolio_nav_is_sum_of_asset_navs() {
        // A buys and rides 100→150 (allocated 5000 → 10 sh*150 = 7500 wait: 5000/100=50 sh -> 7500)
        // B holds cash at 5000. Portfolio end NAV = 7500 + 5000 = 12500.
        let assets = vec![
            PortfolioAsset {
                symbol: "A".into(),
                weight: 1.0,
                candles: candles(&[100.0, 150.0]),
                strategy: Box::new(ScriptedStrategy::new(vec![Signal::Buy, Signal::Hold])),
            },
            PortfolioAsset {
                symbol: "B".into(),
                weight: 1.0,
                candles: candles(&[100.0, 100.0]),
                strategy: Box::new(ScriptedStrategy::new(vec![Signal::Hold, Signal::Hold])),
            },
        ];
        let res = run_portfolio(assets, 10_000.0, ExecutionCosts::default(), None, FillTiming::Close);
        let last = res.equity_curve.last().unwrap().nav;
        assert!((last - 12_500.0).abs() < 1e-6, "last nav = {last}");
        // Aggregate must equal the sum of per-asset last NAVs.
        let sum: f64 = res.assets.iter().map(|a| a.result.equity_curve.last().unwrap().nav).sum();
        assert!((last - sum).abs() < 1e-6);
    }

    #[test]
    fn single_asset_portfolio_matches_direct_engine() {
        // Parity guard: a one-asset, full-weight portfolio equals a direct run.
        // Both use FillTiming::Close to keep the comparison straightforward.
        let closes = [100.0, 110.0, 120.0, 90.0];
        let direct = {
            let strat = ScriptedStrategy::new(vec![Signal::Buy, Signal::Hold, Signal::Sell, Signal::Hold]);
            let pf = Portfolio::new(10_000.0, "X".into());
            let mut eng = BacktestEngine::new(strat, pf).with_fill_timing(FillTiming::Close);
            eng.run(&candles(&closes));
            eng.result()
        };
        let assets = vec![PortfolioAsset {
            symbol: "X".into(),
            weight: 1.0,
            candles: candles(&closes),
            strategy: Box::new(ScriptedStrategy::new(vec![
                Signal::Buy,
                Signal::Hold,
                Signal::Sell,
                Signal::Hold,
            ])),
        }];
        let res = run_portfolio(assets, 10_000.0, ExecutionCosts::default(), None, FillTiming::Close);
        assert!((res.metrics.total_return - direct.metrics.total_return).abs() < 1e-9);
        assert_eq!(res.equity_curve.len(), direct.equity_curve.len());
        assert!((res.equity_curve.last().unwrap().nav - direct.equity_curve.last().unwrap().nav).abs() < 1e-9);
    }

    #[test]
    fn trades_are_concatenated_across_assets() {
        let assets = vec![
            PortfolioAsset {
                symbol: "A".into(),
                weight: 1.0,
                candles: candles(&[100.0, 150.0]),
                strategy: Box::new(ScriptedStrategy::new(vec![Signal::Buy, Signal::Sell])),
            },
            PortfolioAsset {
                symbol: "B".into(),
                weight: 1.0,
                candles: candles(&[100.0, 150.0]),
                strategy: Box::new(ScriptedStrategy::new(vec![Signal::Buy, Signal::Sell])),
            },
        ];
        let res = run_portfolio(assets, 10_000.0, ExecutionCosts::default(), None, FillTiming::Close);
        // Each asset does 1 buy + 1 sell = 2 trades → 4 total.
        assert_eq!(res.metrics.trade_count, 4);
    }

    #[test]
    fn empty_assets_produce_empty_curve() {
        let res = run_portfolio(Vec::new(), 10_000.0, ExecutionCosts::default(), None, FillTiming::Close);
        assert!(res.equity_curve.is_empty());
        assert_eq!(res.metrics.trade_count, 0);
        assert!(res.assets.is_empty());
    }

    #[test]
    fn run_portfolio_populates_risk() {
        // 25 bars → 24 return points, enough for rolling vol (needs 21).
        let n = 25_usize;
        let closes_a: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 0.5).collect();
        let closes_b: Vec<f64> = (0..n).map(|i| 50.0 - i as f64 * 0.2).collect();

        // Buy on bar 1 so NAV tracks price → non-zero returns → non-zero covariance.
        let sig_a: Vec<Signal> = std::iter::once(Signal::Buy)
            .chain(std::iter::repeat(Signal::Hold).take(n - 1))
            .collect();
        let sig_b: Vec<Signal> = std::iter::once(Signal::Buy)
            .chain(std::iter::repeat(Signal::Hold).take(n - 1))
            .collect();

        let assets = vec![
            PortfolioAsset {
                symbol: "A".into(),
                weight: 0.6,
                candles: candles(&closes_a),
                strategy: Box::new(ScriptedStrategy::new(sig_a)),
            },
            PortfolioAsset {
                symbol: "B".into(),
                weight: 0.4,
                candles: candles(&closes_b),
                strategy: Box::new(ScriptedStrategy::new(sig_b)),
            },
        ];
        let res = run_portfolio(assets, 10_000.0, ExecutionCosts::default(), None, FillTiming::Close);

        // Risk struct is 2×2.
        assert_eq!(res.risk.correlation.len(), 2);
        assert_eq!(res.risk.correlation[0].len(), 2);
        assert_eq!(res.risk.contribution_to_risk.len(), 2);

        // Rolling vol: n − 1 returns (24), window 21 → 24 − 21 + 1 = 4 points.
        assert!(!res.risk.rolling_volatility.is_empty());

        // Contributions sum to ~1.
        let ctr_sum: f64 = res.risk.contribution_to_risk.iter().sum();
        assert!((ctr_sum - 1.0).abs() < 1e-9, "CTR sum = {ctr_sum}");
    }

    // ─── Weight policies ─────────────────────────────────────────────────────

    use crate::optimize::{Objective, OptimizerConfig};

    /// Two assets: `a_first`/`a_second` and `b_first`/`b_second` per-bar vol,
    /// switching at `switch_at`. Both are bought and held.
    fn two_asset_run(
        n: usize,
        a: (f64, f64),
        b: (f64, f64),
        switch_at: usize,
        rebalance: Option<RebalanceConfig>,
        policy: WeightPolicy,
    ) -> PortfolioResult {
        let assets = vec![
            PortfolioAsset {
                symbol: "A".into(),
                weight: 1.0,
                candles: long_candles(&switching_prices(n, a.0, a.1, switch_at)),
                strategy: buy_and_hold(n),
            },
            PortfolioAsset {
                symbol: "B".into(),
                weight: 1.0,
                candles: long_candles(&switching_prices(n, b.0, b.1, switch_at)),
                strategy: buy_and_hold(n),
            },
        ];
        run_portfolio_with_policy(
            assets,
            100_000.0,
            ExecutionCosts::default(),
            rebalance,
            FillTiming::Close,
            policy,
        )
    }

    #[test]
    fn manual_policy_is_unchanged_by_the_optimizer_work() {
        // Regression guard: run_portfolio still behaves exactly as before.
        let res = two_asset_run(200, (0.01, 0.01), (0.03, 0.03), 100, None, WeightPolicy::Manual);
        assert!(res.weight_history.is_empty(), "manual must not solve anything");
        assert!(res.rebalance_dates.is_empty(), "no schedule means no rebalances");
        assert!((res.assets[0].weight - 0.5).abs() < 1e-9);
    }

    #[test]
    fn static_policy_solves_exactly_once() {
        let res = two_asset_run(
            300,
            (0.01, 0.01),
            (0.03, 0.03),
            150,
            None,
            WeightPolicy::Static {
                optimizer: OptimizerConfig::new(Objective::MinVariance),
                warmup: 60,
            },
        );
        assert_eq!(res.weight_history.len(), 1, "static solves once, not per bar");
        let snap = &res.weight_history[0];
        assert!((snap.weights.iter().sum::<f64>() - 1.0).abs() < 1e-6);
        assert!(snap.expected_volatility.is_some());
        assert!(snap.risk_contribution.is_some());
    }

    #[test]
    fn static_policy_waits_for_its_warmup() {
        // With a warm-up longer than the data there is nothing to solve on, so
        // the run must fall back to manual weights rather than solving on noise.
        let res = two_asset_run(
            40,
            (0.01, 0.01),
            (0.03, 0.03),
            20,
            None,
            WeightPolicy::Static {
                optimizer: OptimizerConfig::new(Objective::MinVariance),
                warmup: 500,
            },
        );
        assert!(res.weight_history.is_empty());
    }

    #[test]
    fn static_solve_fires_on_the_bar_its_warmup_completes() {
        let n = 300;
        let warmup = 60;
        let res = two_asset_run(
            n,
            (0.01, 0.01),
            (0.03, 0.03),
            150,
            None,
            WeightPolicy::Static {
                optimizer: OptimizerConfig::new(Objective::MinVariance),
                warmup,
            },
        );
        // available = k − 1, so the solve lands on bar index warmup + 1.
        let expected = long_candles(&vec![0.0; n])[warmup + 1].date;
        assert_eq!(res.weight_history[0].date, expected);
        // It also forces a rebalance so the weights take effect immediately.
        assert!(res.rebalance_dates.contains(&expected));
    }

    #[test]
    fn a_static_solve_cannot_see_the_future() {
        // A is calm then wild; B is wild then calm. A min-variance solve at the
        // end of a 60-bar warm-up sees only the calm-A regime, so it must
        // overweight A. Weighting B instead would prove it had read ahead.
        let res = two_asset_run(
            400,
            (0.002, 0.05),
            (0.05, 0.002),
            200,
            None,
            WeightPolicy::Static {
                optimizer: OptimizerConfig { shrinkage: 0.0, ..OptimizerConfig::new(Objective::MinVariance) },
                warmup: 60,
            },
        );
        let w = &res.weight_history[0].weights;
        assert!(w[0] > 0.9, "expected the in-window calm asset to dominate, got {w:?}");
    }

    #[test]
    fn dynamic_policy_resolves_at_every_rebalance() {
        let res = two_asset_run(
            400,
            (0.01, 0.01),
            (0.03, 0.03),
            200,
            Some(RebalanceConfig { frequency: RebalanceFrequency::Monthly }),
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig::new(Objective::RiskParity),
                lookback: 60,
            },
        );
        // ~13 months of daily bars, minus the ones inside the lookback.
        assert!(res.weight_history.len() > 5, "only {} solves", res.weight_history.len());
        assert!(res.weight_history.len() <= res.rebalance_dates.len());
    }

    #[test]
    fn dynamic_weights_track_a_regime_change() {
        // A calm→wild, B wild→calm. Early solves should favour A, late solves B.
        let res = two_asset_run(
            600,
            (0.002, 0.05),
            (0.05, 0.002),
            300,
            Some(RebalanceConfig { frequency: RebalanceFrequency::Monthly }),
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig { shrinkage: 0.0, ..OptimizerConfig::new(Objective::MinVariance) },
                lookback: 60,
            },
        );
        let first = &res.weight_history.first().unwrap().weights;
        let last = &res.weight_history.last().unwrap().weights;
        assert!(first[0] > 0.8, "early solve should favour A, got {first:?}");
        assert!(last[1] > 0.8, "late solve should favour B, got {last:?}");
    }

    #[test]
    fn dynamic_waits_for_a_full_lookback_before_solving() {
        // A partial window is a covariance estimate from a handful of points.
        // Solving on it would move real capital on noise, so every snapshot must
        // come from a complete window.
        let lookback = 100;
        let res = two_asset_run(
            400,
            (0.01, 0.01),
            (0.03, 0.03),
            200,
            Some(RebalanceConfig { frequency: RebalanceFrequency::Monthly }),
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig::new(Objective::RiskParity),
                lookback,
            },
        );
        let dates = long_candles(&vec![0.0; 400]);
        let earliest_valid = dates[lookback + 1].date;
        assert!(!res.weight_history.is_empty());
        assert!(
            res.weight_history[0].date >= earliest_valid,
            "solved on {} with fewer than {lookback} observations",
            res.weight_history[0].date
        );
        // And no solve degenerated to a zero-volatility estimate.
        for snap in &res.weight_history {
            assert!(
                snap.expected_volatility.unwrap_or(0.0) > 0.0,
                "degenerate solve on {}",
                snap.date
            );
        }
    }

    #[test]
    fn dynamic_without_a_schedule_still_resolves() {
        // Dynamic weights only act at rebalance events, so a missing schedule
        // would silently make the policy a no-op. It defaults to monthly.
        let res = two_asset_run(
            400,
            (0.01, 0.01),
            (0.03, 0.03),
            200,
            None,
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig::new(Objective::RiskParity),
                lookback: 60,
            },
        );
        assert!(!res.weight_history.is_empty(), "dynamic must not be a silent no-op");
    }

    #[test]
    fn every_solved_weight_vector_is_a_valid_portfolio() {
        for objective in [
            Objective::EqualWeight,
            Objective::InverseVolatility,
            Objective::MinVariance,
            Objective::RiskParity,
            Objective::MaxSharpe,
        ] {
            let res = two_asset_run(
                400,
                (0.01, 0.015),
                (0.03, 0.008),
                200,
                Some(RebalanceConfig { frequency: RebalanceFrequency::Monthly }),
                WeightPolicy::Dynamic { optimizer: OptimizerConfig::new(objective), lookback: 60 },
            );
            for snap in &res.weight_history {
                let sum: f64 = snap.weights.iter().sum();
                assert!((sum - 1.0).abs() < 1e-6, "{objective:?} on {}: sum {sum}", snap.date);
                assert!(snap.weights.iter().all(|&w| w >= -1e-12), "{objective:?}: {:?}", snap.weights);
            }
        }
    }

    #[test]
    fn weight_history_is_chronological_and_inside_the_backtest() {
        let res = two_asset_run(
            400,
            (0.01, 0.01),
            (0.03, 0.03),
            200,
            Some(RebalanceConfig { frequency: RebalanceFrequency::Monthly }),
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig::new(Objective::RiskParity),
                lookback: 60,
            },
        );
        let first_date = res.equity_curve.first().unwrap().date;
        let last_date = res.equity_curve.last().unwrap().date;
        for pair in res.weight_history.windows(2) {
            assert!(pair[0].date < pair[1].date, "out of order at {}", pair[1].date);
        }
        for snap in &res.weight_history {
            assert!(snap.date >= first_date && snap.date <= last_date);
        }
    }

    #[test]
    fn min_variance_dynamic_beats_equal_weight_on_volatility() {
        // The point of the feature: allocating away from the wild asset should
        // actually reduce realized portfolio volatility.
        let schedule = || Some(RebalanceConfig { frequency: RebalanceFrequency::Monthly });
        let minvar = two_asset_run(
            600,
            (0.002, 0.05),
            (0.05, 0.002),
            300,
            schedule(),
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig { shrinkage: 0.0, ..OptimizerConfig::new(Objective::MinVariance) },
                lookback: 60,
            },
        );
        let equal = two_asset_run(600, (0.002, 0.05), (0.05, 0.002), 300, schedule(), WeightPolicy::Manual);
        assert!(
            minvar.metrics.annualized_volatility < equal.metrics.annualized_volatility,
            "min-var vol {} was not below equal-weight vol {}",
            minvar.metrics.annualized_volatility,
            equal.metrics.annualized_volatility
        );
    }

    #[test]
    fn risk_analytics_describe_the_weights_actually_in_force() {
        // Under a solving policy the final allocation is not the manual one, so
        // contribution-to-risk must be computed against the solved targets.
        let res = two_asset_run(
            400,
            (0.002, 0.002),
            (0.05, 0.05),
            200,
            None,
            WeightPolicy::Static {
                optimizer: OptimizerConfig { shrinkage: 0.0, ..OptimizerConfig::new(Objective::MinVariance) },
                warmup: 60,
            },
        );
        let ctr_sum: f64 = res.risk.contribution_to_risk.iter().sum();
        assert!((ctr_sum - 1.0).abs() < 1e-9);
        // The calm asset holds nearly all the capital, so it carries the risk.
        assert!(res.risk.contribution_to_risk[0] > res.risk.contribution_to_risk[1]);
    }

    #[test]
    fn an_asset_driven_to_zero_weight_can_be_re_funded() {
        // Regression: tracking rebalances as accumulated multiplicative ratios
        // meant an asset the optimizer zeroed out could never come back —
        // restoring it needed `target / ≈0`, which overflowed to infinity and
        // turned the whole equity curve into NaN. A calm→wild / wild→calm pair
        // under min-variance drives exactly that: B is dropped, then wanted.
        let res = two_asset_run(
            600,
            (0.002, 0.05),
            (0.05, 0.002),
            300,
            Some(RebalanceConfig { frequency: RebalanceFrequency::Monthly }),
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig { shrinkage: 0.0, ..OptimizerConfig::new(Objective::MinVariance) },
                lookback: 60,
            },
        );

        assert!(
            res.equity_curve.iter().all(|p| p.nav.is_finite()),
            "equity curve went non-finite"
        );
        assert!(res.metrics.annualized_volatility.is_finite());
        assert!(res.metrics.total_return.is_finite());

        // The scenario really does zero an asset out and later want it back.
        let min_b = res.weight_history.iter().map(|s| s.weights[1]).fold(f64::MAX, f64::min);
        let max_b = res.weight_history.iter().map(|s| s.weights[1]).fold(f64::MIN, f64::max);
        assert!(min_b < 0.05, "B was never dropped (min {min_b})");
        assert!(max_b > 0.8, "B was never restored (max {max_b})");
    }

    #[test]
    fn a_single_asset_portfolio_is_unaffected_by_the_policy() {
        let assets = vec![PortfolioAsset {
            symbol: "X".into(),
            weight: 1.0,
            candles: long_candles(&switching_prices(200, 0.01, 0.01, 100)),
            strategy: buy_and_hold(200),
        }];
        let res = run_portfolio_with_policy(
            assets,
            10_000.0,
            ExecutionCosts::default(),
            None,
            FillTiming::Close,
            WeightPolicy::Static {
                optimizer: OptimizerConfig::new(Objective::MinVariance),
                warmup: 30,
            },
        );
        for snap in &res.weight_history {
            assert!((snap.weights[0] - 1.0).abs() < 1e-9, "{:?}", snap.weights);
        }
    }
}
