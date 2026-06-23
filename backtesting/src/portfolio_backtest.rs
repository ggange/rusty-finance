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

    let (equity_curve, metrics, benchmark, risk, rebalance_dates) =
        aggregate(&asset_results, initial_cash, rebalance.as_ref(), &costs);
    PortfolioResult { equity_curve, metrics, benchmark, risk, assets: asset_results, rebalance_dates }
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
) -> (Vec<EquityPoint>, Metrics, Benchmark, RiskMetrics, Vec<NaiveDate>) {
    // Union of every date that appears in any asset's curve, sorted.
    let mut dates: BTreeMap<NaiveDate, ()> = BTreeMap::new();
    for a in assets {
        for ep in &a.result.equity_curve {
            dates.insert(ep.date, ());
        }
    }

    // Per-asset NAV adjustment multipliers: updated on each rebalance event.
    // multiplier[i] accumulates the scaling applied by all rebalances so far.
    let mut multipliers: Vec<f64> = vec![1.0; assets.len()];
    let mut rebalance_dates: Vec<NaiveDate> = Vec::new();
    let mut prev_date: Option<NaiveDate> = None;

    let mut equity_curve: Vec<EquityPoint> = Vec::with_capacity(dates.len());
    for &date in dates.keys() {
        // Compute each asset's adjusted NAV at this date.
        let adj_navs: Vec<f64> = assets
            .iter()
            .enumerate()
            .map(|(i, a)| asof_nav(&a.result.equity_curve, date, a.allocated_cash) * multipliers[i])
            .collect();
        let total: f64 = adj_navs.iter().sum();

        // Check whether a rebalance should fire on this date.
        if let (Some(cfg), Some(prev)) = (rebalance, prev_date) {
            let fires = match &cfg.frequency {
                RebalanceFrequency::Monthly => {
                    date.month() != prev.month() || date.year() != prev.year()
                }
                RebalanceFrequency::Quarterly => {
                    let q = |d: NaiveDate| (d.month() - 1) / 3;
                    q(date) != q(prev) || date.year() != prev.year()
                }
                RebalanceFrequency::Threshold { threshold } => {
                    if total > 0.0 {
                        assets.iter().enumerate().any(|(i, a)| {
                            let actual = adj_navs[i] / total;
                            (actual - a.weight).abs() > *threshold
                        })
                    } else {
                        false
                    }
                }
            };

            if fires && total > 0.0 {
                // Execution cost: slippage on each leg + commission per asset traded.
                let slippage_cost: f64 = assets
                    .iter()
                    .enumerate()
                    .map(|(i, a)| (adj_navs[i] - total * a.weight).abs() * costs.slippage_pct)
                    .sum();
                let commission_cost = costs.commission_per_trade * assets.len() as f64;
                let total_after_costs = (total - slippage_cost - commission_cost).max(0.0);

                // Update multipliers: scale each asset so its new NAV equals target.
                for (i, a) in assets.iter().enumerate() {
                    let current_adj_nav = adj_navs[i];
                    let target_nav = total_after_costs * a.weight;
                    if current_adj_nav > 0.0 {
                        multipliers[i] *= target_nav / current_adj_nav;
                    }
                }
                rebalance_dates.push(date);
            }
        }

        // Recompute totals after any rebalance multiplier updates.
        let nav: f64 = assets
            .iter()
            .enumerate()
            .map(|(i, a)| asof_nav(&a.result.equity_curve, date, a.allocated_cash) * multipliers[i])
            .sum();
        equity_curve.push(EquityPoint { date, nav });
        prev_date = Some(date);
    }

    // Aligned per-asset daily return series for risk analytics.
    let dates_vec: Vec<NaiveDate> = dates.keys().copied().collect();
    let asset_returns: Vec<Vec<f64>> = assets
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let navs: Vec<f64> = dates_vec
                .iter()
                .map(|&d| asof_nav(&a.result.equity_curve, d, a.allocated_cash) * multipliers[i])
                .collect();
            navs.windows(2)
                .map(|w| if w[0] != 0.0 { w[1] / w[0] - 1.0 } else { 0.0 })
                .collect()
        })
        .collect();
    let symbols: Vec<String> = assets.iter().map(|a| a.symbol.clone()).collect();
    let weights: Vec<f64> = assets.iter().map(|a| a.weight).collect();
    let risk = RiskMetrics::compute(&symbols, &asset_returns, &weights, &equity_curve);

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

    (equity_curve, metrics, benchmark, risk, rebalance_dates)
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
}
