//! Walk-forward validation: for each rolling fold, train on the first
//! `train_frac` of the fold and test on the remainder. The best parameter
//! combination (by `metric`) from the train slice is then scored on the test
//! slice.
//!
//! Two properties this module exists to protect:
//!
//! - **Selection never sees the test slice.** The grid is scored on `train`
//!   only. Where the metric cannot tell combos apart, the fold reports
//!   `tied_candidates` rather than pretending a choice was made.
//! - **The test slice is warm-started, not restarted.** The chosen strategy runs
//!   across the whole fold and is scored from the first test bar onward, so its
//!   indicators are primed exactly as a live deployment's would be on that date.
//!   Every bar it has seen still precedes the scored window, so there is no
//!   look-ahead.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::bootstrap::BootstrapConfig;
use crate::data::Candle;
use crate::engine::{BacktestEngine, FillTiming};
use crate::metrics::Metrics;
use crate::portfolio::{EquityPoint, ExecutionCosts, Portfolio, TradeRecord};
use crate::strategy::ma::{MAType, MovingAverageCrossover};
use crate::strategy::macd::MACD;
use crate::strategy::rsi::RSI;
use crate::strategy::bollinger_bands::BollingerBands as BBands;
use crate::strategy::Strategy;

/// Date range (inclusive on both ends) for one fold's train or test slice.
#[derive(Debug, Serialize)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// Result for one fold in the walk-forward run.
#[derive(Debug, Serialize)]
pub struct WalkForwardFold {
    pub window_index: usize,
    pub train_range: DateRange,
    pub test_range: DateRange,
    /// The parameter combination that scored highest on the train slice.
    pub best_params: HashMap<String, Value>,
    /// Metrics from the best combo run on the train slice.
    pub train_metrics: Metrics,
    /// Metrics from the same combo run on the test slice, warm-started from the
    /// train slice.
    pub test_metrics: Metrics,
    /// How many grid combos matched the winning train score.
    ///
    /// `1` is a clean win. Anything higher means the metric could not
    /// discriminate between that many parameter sets and the tie was broken by
    /// a secondary rule — so `best_params` was not really *chosen*, and a
    /// per-fold parameter comparison across folds is reading noise. `0` means
    /// nothing was selectable (every combo scored NaN). Ties are common,
    /// because any strategy that never fires on the train slice scores exactly
    /// 0.0, and they used to resolve silently to the first combo in the grid.
    pub tied_candidates: usize,
}

/// Output of a full walk-forward run.
#[derive(Debug, Serialize)]
pub struct WalkForwardResult {
    pub folds: Vec<WalkForwardFold>,
    /// Metrics for every fold's out-of-sample returns stitched into one series,
    /// as though the walk-forward procedure had been traded continuously.
    ///
    /// This is the honest headline for a walk-forward run, and it is not the
    /// average of the per-fold metrics. Averaging folds treats each as an
    /// independent observation; pooling treats the sequence as the single
    /// dependent return path it actually is, which is what
    /// `docs/strategy-validation.md` argues the numbers should be read as.
    ///
    /// `None` when no fold produced at least two out-of-sample returns.
    pub oos_metrics: Option<Metrics>,
}

/// Run walk-forward validation.
///
/// Splits `candles` into `n_windows` rolling folds of equal length. Each fold
/// uses the first `train_frac` of its bars as the train slice and the rest as
/// the test slice. For each fold the full `strategy_grid` is evaluated on the
/// train slice; the combo with the best `metric` score is then scored on the
/// test slice, warm-started from the train slice, and both metric sets are
/// returned along with `tied_candidates`.
///
/// The train slice must be long enough to warm up the widest indicator in the
/// grid, or every combo scores 0.0 and the selection is a tie. The *test* slice
/// has no such requirement, because it inherits warm indicator state.
///
/// # Errors
/// Returns a descriptive `String` on invalid inputs (too few candles, invalid
/// metric name, etc.) so the Python caller can raise a `PyValueError`.
pub fn run_walk_forward(
    candles: &[Candle],
    strategy_grid: &[Value],
    n_windows: usize,
    train_frac: f64,
    metric: &str,
    initial_cash: f64,
    costs: ExecutionCosts,
    fill_timing: FillTiming,
    bootstrap: &BootstrapConfig,
) -> Result<WalkForwardResult, String> {
    if n_windows < 2 {
        return Err(format!("n_windows must be >= 2, got {n_windows}"));
    }
    if !(train_frac > 0.0 && train_frac < 1.0) {
        return Err(format!("train_frac must be in (0, 1), got {train_frac}"));
    }
    if strategy_grid.is_empty() {
        return Err("strategy_grid is empty".to_string());
    }
    let n = candles.len();
    if n < n_windows * 2 {
        return Err(format!(
            "not enough candles ({n}) for {n_windows} windows (need at least {})",
            n_windows * 2
        ));
    }

    let fold_len = n / n_windows;
    let mut folds: Vec<WalkForwardFold> = Vec::with_capacity(n_windows);
    // Every fold's out-of-sample returns, in chronological order, so the run can
    // report one bounded number for the procedure rather than only per-fold ones.
    let mut pooled_oos: Vec<f64> = Vec::with_capacity(n);

    for wi in 0..n_windows {
        let fold_start = wi * fold_len;
        // Last window absorbs any leftover bars.
        let fold_end = if wi + 1 == n_windows { n } else { fold_start + fold_len };
        let fold = &candles[fold_start..fold_end];

        let train_len = ((fold.len() as f64) * train_frac).floor() as usize;
        let train_len = train_len.max(1).min(fold.len() - 1);
        let train = &fold[..train_len];
        let test = &fold[train_len..];

        if test.is_empty() {
            return Err(format!("fold {wi}: test slice is empty; reduce train_frac or n_windows"));
        }

        // Run the entire grid on the train slice. Selection reads `train` only —
        // this is the in-sample score, and the test slice must stay unseen.
        let mut candidates: Vec<(f64, usize)> = Vec::with_capacity(strategy_grid.len());
        let mut train_metrics_vec: Vec<Metrics> = Vec::with_capacity(strategy_grid.len());

        for spec_val in strategy_grid.iter() {
            let m = run_one(spec_val, train, initial_cash, &costs, fill_timing)?;
            candidates.push((metric_score(&m, metric), m.trade_count));
            train_metrics_vec.push(m);
        }

        let (best_idx, tied_candidates) = select_best(&candidates);

        // Score the best combo on the test slice, warm-started from the train
        // slice so the indicators are primed the way a live deployment's would be.
        let best_spec = &strategy_grid[best_idx];
        let (test_metrics, test_returns) = run_test_warm_started(
            best_spec, fold, train_len, initial_cash, &costs, fill_timing, bootstrap,
        )?;
        pooled_oos.extend(test_returns);
        let train_metrics = train_metrics_vec.remove(best_idx);

        // Extract best_params (all fields except "type").
        let best_params: HashMap<String, Value> = best_spec
            .as_object()
            .map(|obj| {
                obj.iter()
                    .filter(|(k, _)| k.as_str() != "type")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();

        folds.push(WalkForwardFold {
            window_index: wi,
            train_range: DateRange {
                start: train.first().unwrap().date,
                end: train.last().unwrap().date,
            },
            test_range: DateRange {
                start: test.first().unwrap().date,
                end: test.last().unwrap().date,
            },
            best_params,
            train_metrics,
            test_metrics,
            tied_candidates,
        });
    }

    let oos_metrics = pool_out_of_sample(&pooled_oos, initial_cash, bootstrap);
    Ok(WalkForwardResult { folds, oos_metrics })
}

/// Metrics for the stitched out-of-sample return path, with its own interval.
///
/// Compounds every fold's test returns into one curve, as though the walk-forward
/// procedure had been traded continuously. The NAV levels are synthetic — capital
/// is not really reset between folds — but every metric here is a function of the
/// return sequence alone, so the level only sets the scale.
fn pool_out_of_sample(
    returns: &[f64],
    initial_cash: f64,
    bootstrap: &BootstrapConfig,
) -> Option<Metrics> {
    if returns.len() < 2 {
        return None;
    }
    // Dates are placeholders: no metric reads them, a property pinned by
    // `metrics_are_invariant_to_the_curve_dates`.
    let base = NaiveDate::from_ymd_opt(2000, 1, 1)?;
    let mut nav = initial_cash;
    let mut curve = Vec::with_capacity(returns.len() + 1);
    curve.push(EquityPoint { date: base, nav });
    for (i, r) in returns.iter().enumerate() {
        nav *= 1.0 + r;
        curve.push(EquityPoint {
            date: base + chrono::Duration::days(i as i64 + 1),
            nav,
        });
    }
    // No trade log: win rate and trade count belong to the folds, not to a
    // stitched path that never existed as a single backtest.
    Some(Metrics::compute(&curve, &[]).with_uncertainty(&curve, bootstrap))
}

/// Pick the winning grid index from `(score, trade_count)` pairs.
///
/// Returns `(best_index, tied_candidates)`, where `tied_candidates` counts how
/// many combos matched the winning score — `1` for a clean win, `0` when nothing
/// was selectable because every score was NaN.
///
/// Two rules beyond "highest score wins":
///
/// - **NaN never wins.** A combo whose metric came back NaN is not a candidate.
/// - **At an exact tie, a combo that traded beats one that did not.** A 0.0
///   Sharpe from a strategy that never fired carries no evidence; a 0.0 from one
///   that round-tripped to breakeven does.
///
/// Ties still have to resolve to *something*, and beyond the trade-count rule
/// that something is the lowest grid index — which is arbitrary. That is
/// precisely why the count is returned and surfaced on the fold rather than
/// discarded: a fold reporting `tied_candidates > 1` did not really select its
/// parameters, and comparing `best_params` across such folds is reading noise.
fn select_best(candidates: &[(f64, usize)]) -> (usize, usize) {
    let mut best: Option<(usize, f64, usize)> = None;
    for (i, &(score, trades)) in candidates.iter().enumerate() {
        if score.is_nan() {
            continue;
        }
        let better = match best {
            None => true,
            // Strictly higher score, or an equal score from a combo that
            // actually traded where the incumbent did not.
            Some((_, bs, bt)) => score > bs || (score == bs && trades > 0 && bt == 0),
        };
        if better {
            best = Some((i, score, trades));
        }
    }
    match best {
        None => (0, 0),
        Some((idx, score, _)) => {
            let tied = candidates.iter().filter(|&&(s, _)| s == score).count();
            (idx, tied)
        }
    }
}

/// Run `spec` over the whole fold but score only the test slice.
///
/// The strategy sees `train` before `test`, so its indicators are already warm
/// when the test window opens — the same state a live deployment would have on
/// that date, and no look-ahead, because every bar it has seen precedes the test
/// window. Cold-starting the strategy on `test` instead makes it pay a second
/// warm-up: the scored window is silently truncated by the indicator's period,
/// and when the test slice is shorter than that period the result is guaranteed
/// zero trades and an all-zero metric set presented as an out-of-sample score.
///
/// Only bars from `train_len` onward are scored. [`Metrics`] normalises by the
/// first NAV of the curve it is handed, so slicing rebases the test window
/// automatically.
///
/// One asymmetry worth knowing: a position opened late in `train` carries into
/// the test window, so the test slice is not always entered flat the way the
/// train slice is. That is realistic — it is what a live system would hold on
/// that date — but it means a test fold can book the exit of a trade whose entry
/// was decided on train data.
fn run_test_warm_started(
    spec_val: &Value,
    fold: &[Candle],
    train_len: usize,
    initial_cash: f64,
    costs: &ExecutionCosts,
    fill_timing: FillTiming,
    bootstrap: &BootstrapConfig,
) -> Result<(Metrics, Vec<f64>), String> {
    let strategy = build_strategy_from_value(spec_val)?;
    let portfolio = Portfolio::new(initial_cash, "WF".to_string()).with_costs(costs.clone());
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(fill_timing);
    engine.run(fold);
    let result = engine.result();

    let test_start = fold[train_len].date;
    let curve: Vec<EquityPoint> = result
        .equity_curve
        .into_iter()
        .filter(|p| p.date >= test_start)
        .collect();
    let trades: Vec<TradeRecord> = result
        .trades
        .into_iter()
        .filter(|t| t.date >= test_start)
        .collect();

    // Returned alongside the metrics so the caller can stitch every fold's
    // out-of-sample returns into one series and bound *that*.
    let returns: Vec<f64> = curve.windows(2).map(|w| w[1].nav / w[0].nav - 1.0).collect();

    let metrics = Metrics::compute(&curve, &trades).with_uncertainty(&curve, bootstrap);
    Ok((metrics, returns))
}

/// Run one strategy spec on a candle slice and return its metrics.
fn run_one(
    spec_val: &Value,
    candles: &[Candle],
    initial_cash: f64,
    costs: &ExecutionCosts,
    fill_timing: FillTiming,
) -> Result<Metrics, String> {
    let strategy = build_strategy_from_value(spec_val)?;
    let portfolio = Portfolio::new(initial_cash, "WF".to_string()).with_costs(costs.clone());
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(fill_timing);
    engine.run(candles);
    Ok(engine.result().metrics)
}

/// Extract the chosen metric from a `Metrics` struct for ranking.
/// `max_drawdown` is returned as its absolute value so that lower (better) draws
/// rank lower — the caller selects `score > best_score` for all metrics.
/// For metrics where lower is better (just max_drawdown), we negate so the
/// "highest score wins" logic still works.
fn metric_score(m: &Metrics, metric: &str) -> f64 {
    match metric {
        "total_return"          => m.total_return,
        "cagr"                  => m.cagr,
        "annualized_volatility" => -m.annualized_volatility, // lower is better
        "max_drawdown"          => -m.max_drawdown.abs(),    // less negative = better (smaller drawdown)
        "sharpe_ratio"          => m.sharpe_ratio,
        "sortino_ratio"         => m.sortino_ratio,
        "win_rate"              => m.win_rate.unwrap_or(f64::NEG_INFINITY),
        other => {
            // Unknown metric: log and fall back to sharpe_ratio.
            eprintln!("walk_forward: unknown metric {:?}, falling back to sharpe_ratio", other);
            m.sharpe_ratio
        }
    }
}

// ─── Strategy builder (mirrors backtesting-py/src/lib.rs but returns String errors) ──

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StrategySpec {
    MaEma { short_window: usize, long_window: usize },
    MaSma { short_window: usize, long_window: usize },
    MaWma { short_window: usize, long_window: usize },
    Rsi    { period: usize },
    Macd   { fast_period: usize, slow_period: usize, signal_period: usize },
    BollingerBands { period: usize, std_dev_mult: f64 },
}

fn build_strategy_from_value(v: &Value) -> Result<Box<dyn Strategy>, String> {
    let spec: StrategySpec = serde_json::from_value(v.clone())
        .map_err(|e| format!("invalid strategy spec: {e}"))?;
    build_strategy(spec)
}

fn build_strategy(spec: StrategySpec) -> Result<Box<dyn Strategy>, String> {
    Ok(match spec {
        StrategySpec::MaEma { short_window, long_window } => {
            validate_ma(short_window, long_window)?;
            Box::new(MovingAverageCrossover::new(MAType::EMA, short_window, long_window))
        }
        StrategySpec::MaSma { short_window, long_window } => {
            validate_ma(short_window, long_window)?;
            Box::new(MovingAverageCrossover::new(MAType::SMA, short_window, long_window))
        }
        StrategySpec::MaWma { short_window, long_window } => {
            validate_ma(short_window, long_window)?;
            Box::new(MovingAverageCrossover::new(MAType::WMA, short_window, long_window))
        }
        StrategySpec::Rsi { period } => {
            if period < 2 { return Err("RSI period must be >= 2".to_string()); }
            Box::new(RSI::new(period))
        }
        StrategySpec::Macd { fast_period, slow_period, signal_period } => {
            if fast_period < 2 || slow_period < 2 || signal_period < 2 {
                return Err("MACD periods must be >= 2".to_string());
            }
            if fast_period >= slow_period {
                return Err(format!("fast_period ({fast_period}) must be < slow_period ({slow_period})"));
            }
            Box::new(MACD::new(fast_period, slow_period, signal_period))
        }
        StrategySpec::BollingerBands { period, std_dev_mult } => {
            if period < 2 { return Err("Bollinger Bands period must be >= 2".to_string()); }
            if std_dev_mult <= 0.0 { return Err("std_dev_mult must be > 0".to_string()); }
            Box::new(BBands::new(period, std_dev_mult))
        }
    })
}

fn validate_ma(short: usize, long: usize) -> Result<(), String> {
    if short == 0 || long == 0 {
        return Err("MA windows must be > 0".to_string());
    }
    if short >= long {
        return Err(format!("short_window ({short}) must be less than long_window ({long})"));
    }
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_candle(i: usize, price: f64) -> Candle {
        // Use a base date and add `i` days to stay within valid calendar bounds.
        let base = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let date = base + chrono::Duration::days(i as i64);
        Candle { date, open: price, high: price, low: price, close: price, volume: 0 }
    }

    fn synthetic_candles(n: usize) -> Vec<Candle> {
        (0..n).map(|i| make_candle(i, 100.0 + i as f64 * 0.5)).collect()
    }

    fn simple_grid() -> Vec<Value> {
        serde_json::from_str(
            r#"[
                {"type": "ma_ema", "short_window": 3, "long_window": 5},
                {"type": "ma_ema", "short_window": 5, "long_window": 10}
            ]"#
        ).unwrap()
    }

    /// Oscillating prices, so a short/long MA pair crosses repeatedly.
    fn oscillating_candles(n: usize) -> Vec<Candle> {
        (0..n)
            .map(|i| make_candle(i, 100.0 + 10.0 * (i as f64 / 2.5).sin()))
            .collect()
    }

    // ─── Selection ───────────────────────────────────────────────────────────

    #[test]
    fn a_clean_win_reports_one_candidate() {
        let (idx, tied) = select_best(&[(0.5, 3), (1.2, 4), (0.9, 2)]);
        assert_eq!(idx, 1);
        assert_eq!(tied, 1);
    }

    #[test]
    fn ties_are_counted_not_hidden() {
        // Three combos that all scored 0.0 because none of them ever fired.
        // "Best params" is meaningless here and the caller has to be able to see
        // that, rather than being handed grid index 0 as though it had won.
        let (idx, tied) = select_best(&[(0.0, 0), (0.0, 0), (0.0, 0)]);
        assert_eq!(idx, 0);
        assert_eq!(tied, 3);
    }

    #[test]
    fn a_tie_prefers_the_combo_that_actually_traded() {
        // Grid order must not decide this: a 0.0 from a strategy that never
        // fired is weaker evidence than a 0.0 from one that round-tripped.
        let (idx, tied) = select_best(&[(0.0, 0), (0.0, 6)]);
        assert_eq!(idx, 1);
        // Still reported as a tie — the *metric* could not discriminate.
        assert_eq!(tied, 2);
    }

    #[test]
    fn nan_scores_are_never_selected() {
        let (idx, _) = select_best(&[(f64::NAN, 5), (0.3, 2)]);
        assert_eq!(idx, 1);
    }

    #[test]
    fn nothing_selectable_reports_zero_candidates() {
        let (idx, tied) = select_best(&[(f64::NAN, 0), (f64::NAN, 0)]);
        assert_eq!(idx, 0);
        assert_eq!(tied, 0);
    }

    #[test]
    fn a_grid_that_never_fires_reports_the_tie_on_the_fold() {
        // Every combo needs more bars than the train slice has, so all score
        // 0.0. The fold must admit that rather than presenting the first combo
        // as a validated winner.
        let candles = oscillating_candles(100);
        let grid: Vec<Value> = serde_json::from_str(
            r#"[
                {"type": "ma_sma", "short_window": 2, "long_window": 40},
                {"type": "ma_sma", "short_window": 3, "long_window": 45}
            ]"#,
        )
        .unwrap();
        let result = run_walk_forward(
            &candles, &grid, 2, 0.7, "sharpe_ratio",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::off(),
        )
        .expect("should succeed");

        let fold = &result.folds[0];
        assert_eq!(fold.train_metrics.trade_count, 0, "train slice should be too short to trade");
        assert_eq!(fold.tied_candidates, 2, "both combos scored 0.0 and the tie must be reported");
    }

    // ─── Uncertainty ─────────────────────────────────────────────────────────

    fn wf_with_intervals(candles: &[Candle]) -> WalkForwardResult {
        let grid: Vec<Value> = serde_json::from_str(
            r#"[{"type": "ma_sma", "short_window": 2, "long_window": 10}]"#,
        )
        .unwrap();
        run_walk_forward(
            candles, &grid, 3, 0.7, "sharpe_ratio",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::default(),
        )
        .expect("should succeed")
    }

    #[test]
    fn walk_forward_bounds_the_test_fold_but_not_the_train_fold() {
        // The train metric is an arg-max over the grid; an interval around a
        // selected maximum has no valid frequentist reading, and `run_one` is the
        // per-cell hot path besides. Only the out-of-sample number gets bounded.
        let result = wf_with_intervals(&oscillating_candles(300));
        for fold in &result.folds {
            assert!(
                fold.test_metrics.uncertainty.is_some(),
                "fold {} test metrics should carry an interval",
                fold.window_index
            );
            assert!(
                fold.train_metrics.uncertainty.is_none(),
                "fold {} train metrics must not be bounded",
                fold.window_index
            );
        }
    }

    #[test]
    fn the_pooled_out_of_sample_interval_is_narrower_than_the_folds_it_pools() {
        // The point of pooling: three ~30-bar test windows bounded separately are
        // each hopelessly wide, while the same returns as one ~90-bar series
        // support a tighter statement. This is the number
        // `docs/strategy-validation.md` argues should be the headline, in place of
        // an average over folds treated as independent observations.
        let result = wf_with_intervals(&oscillating_candles(300));
        let pooled = result.oos_metrics.as_ref().expect("pooled metrics").uncertainty.as_ref().unwrap();

        let pooled_width = pooled.sharpe_ratio.hi - pooled.sharpe_ratio.lo;
        for fold in &result.folds {
            let u = fold.test_metrics.uncertainty.as_ref().unwrap();
            let fold_width = u.sharpe_ratio.hi - u.sharpe_ratio.lo;
            assert!(
                pooled_width < fold_width,
                "pooled width {pooled_width:.3} should beat fold {} width {fold_width:.3}",
                fold.window_index
            );
        }
        assert!(pooled.observations > result.folds[0].test_metrics.uncertainty.as_ref().unwrap().observations);
    }

    #[test]
    fn the_pooled_path_holds_every_folds_out_of_sample_returns() {
        // Guards the stitching: the pooled series must be the concatenation of
        // the per-fold test windows, not a resample of one of them.
        let result = wf_with_intervals(&oscillating_candles(300));
        let pooled = result.oos_metrics.as_ref().unwrap().uncertainty.as_ref().unwrap();
        let per_fold: usize = result
            .folds
            .iter()
            .map(|f| f.test_metrics.uncertainty.as_ref().unwrap().observations)
            .sum();
        assert_eq!(pooled.observations, per_fold);
    }

    #[test]
    fn a_disabled_config_leaves_every_fold_and_the_pool_unbounded() {
        let result = run_walk_forward(
            &oscillating_candles(300),
            &serde_json::from_str::<Vec<Value>>(
                r#"[{"type": "ma_sma", "short_window": 2, "long_window": 10}]"#,
            )
            .unwrap(),
            3, 0.7, "sharpe_ratio",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::off(),
        )
        .unwrap();
        assert!(result.folds.iter().all(|f| f.test_metrics.uncertainty.is_none()));
        // The pooled metrics still exist — they are a point estimate like any
        // other. Only the interval is withheld.
        assert!(result.oos_metrics.is_some());
        assert!(result.oos_metrics.unwrap().uncertainty.is_none());
    }

    // ─── Warm start ──────────────────────────────────────────────────────────

    #[test]
    fn the_test_slice_inherits_indicator_state_from_train() {
        // fold_len 50 → train 35, test 15. A 20-bar SMA cannot form inside a
        // 15-bar slice, so cold-starting the strategy on the test window
        // guarantees zero trades regardless of what prices do. Warm-started from
        // the 35 train bars it is already primed and trades immediately.
        let candles = oscillating_candles(100);
        let grid: Vec<Value> = serde_json::from_str(
            r#"[{"type": "ma_sma", "short_window": 2, "long_window": 20}]"#,
        )
        .unwrap();
        let result = run_walk_forward(
            &candles, &grid, 2, 0.7, "total_return",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::off(),
        )
        .expect("should succeed");

        for fold in &result.folds {
            assert!(
                fold.test_metrics.trade_count > 0,
                "fold {} scored no trades out of sample — cold start",
                fold.window_index
            );
        }
    }

    #[test]
    fn a_cold_run_shorter_than_the_indicator_cannot_trade() {
        // The whole reason `run_test_warm_started` exists, as a direct A/B on the
        // same spec and the same bars: 15 bars cannot form a 20-bar SMA, so a
        // cold run scores zero trades whatever the prices do. That is what test
        // slices used to be scored on, and it is indistinguishable in the output
        // from a strategy that legitimately found nothing to do.
        let candles = oscillating_candles(100);
        let spec: Value = serde_json::from_str(
            r#"{"type": "ma_sma", "short_window": 2, "long_window": 20}"#,
        )
        .unwrap();
        let costs = ExecutionCosts::default();

        let cold = run_one(&spec, &candles[35..50], 10_000.0, &costs, FillTiming::Close)
            .expect("cold run should succeed");
        assert_eq!(cold.trade_count, 0, "a cold 15-bar slice cannot trade a 20-bar SMA");

        let (warm, warm_returns) = run_test_warm_started(
            &spec, &candles[0..50], 35, 10_000.0, &costs, FillTiming::Close,
            &BootstrapConfig::off(),
        )
        .expect("warm run should succeed");
        assert!(
            warm.trade_count > 0,
            "warm-started run should trade the same slice, got {}",
            warm.trade_count
        );
        // The pooled out-of-sample path is stitched from exactly these returns,
        // one per scored test bar.
        assert_eq!(warm_returns.len(), 14, "15 test bars yield 14 returns");
    }

    #[test]
    fn test_metrics_are_rebased_to_the_test_window() {
        // Prices climb through the train slice, then go flat for the whole test
        // slice. Warm-starting runs the engine across the entire fold, so the
        // test metrics must still be measured from the first test bar: a flat
        // test window is a 0 % return even though the fold as a whole gained.
        // Zero-cost execution, so trades inside the flat stretch cannot move NAV.
        // Rising *and* oscillating through the train slice, so the MA pair
        // actually crosses and trades — a monotonic ramp never crosses at all.
        let wave = |i: f64| 100.0 + 0.8 * i + 8.0 * (i / 2.5).sin();
        let candles: Vec<Candle> = (0..100)
            .map(|i| {
                let price = if i < 35 {
                    wave(i as f64)
                } else if i < 50 {
                    wave(34.0) // flat for the whole test slice
                } else {
                    wave(i as f64 - 15.0)
                };
                make_candle(i, price)
            })
            .collect();
        let grid: Vec<Value> = serde_json::from_str(
            r#"[{"type": "ma_sma", "short_window": 2, "long_window": 10}]"#,
        )
        .unwrap();
        let result = run_walk_forward(
            &candles, &grid, 2, 0.7, "total_return",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::off(),
        )
        .expect("should succeed");

        let fold = &result.folds[0];
        assert!(
            fold.train_metrics.total_return > 0.0,
            "train slice should have gained, got {}",
            fold.train_metrics.total_return
        );
        assert!(
            fold.test_metrics.total_return.abs() < 1e-9,
            "flat test window should be a 0 % return, got {}",
            fold.test_metrics.total_return
        );
    }

    #[test]
    fn folds_partition_in_order() {
        // 30 candles, 3 windows → fold_len = 10; slices must not overlap and
        // must be contiguous (train_range.end < test_range.start is OK because
        // train ends one bar before test starts within the fold).
        let candles = synthetic_candles(30);
        let grid = simple_grid();
        let result = run_walk_forward(
            &candles, &grid, 3, 0.7, "sharpe_ratio",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::off(),
        ).expect("should succeed");

        assert_eq!(result.folds.len(), 3);

        // Each fold's train_range.start must be before its test_range.start.
        for fold in &result.folds {
            assert!(fold.train_range.start <= fold.train_range.end);
            assert!(fold.test_range.start <= fold.test_range.end);
            assert!(fold.train_range.end < fold.test_range.start);
        }

        // Folds are in chronological order.
        for w in result.folds.windows(2) {
            assert!(w[0].test_range.end < w[1].train_range.start,
                "fold {} test end {:?} must precede fold {} train start {:?}",
                w[0].window_index, w[0].test_range.end,
                w[1].window_index, w[1].train_range.start);
        }
    }

    #[test]
    fn best_params_come_from_grid() {
        let candles = synthetic_candles(60);
        let grid = simple_grid();
        let result = run_walk_forward(
            &candles, &grid, 3, 0.7, "sharpe_ratio",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::off(),
        ).expect("should succeed");

        // Every fold's best_params must have short_window and long_window from the grid.
        let valid_combos: Vec<(i64, i64)> = vec![(3, 5), (5, 10)];
        for fold in &result.folds {
            let sw = fold.best_params["short_window"].as_i64()
                .or_else(|| fold.best_params["short_window"].as_f64().map(|v| v as i64))
                .expect("short_window must be present");
            let lw = fold.best_params["long_window"].as_i64()
                .or_else(|| fold.best_params["long_window"].as_f64().map(|v| v as i64))
                .expect("long_window must be present");
            let valid = valid_combos.iter().any(|(s, l)| sw == *s && lw == *l);
            assert!(valid, "best_params short_window={sw} long_window={lw} not in grid");
        }
    }

    #[test]
    fn slice_lengths_match_n_windows_and_train_frac() {
        // 30 candles, 3 windows → fold_len = 10; train_frac = 0.7 → 7 train bars, 3 test bars.
        let candles = synthetic_candles(30);
        let grid = simple_grid();
        let result = run_walk_forward(
            &candles, &grid, 3, 0.7, "sharpe_ratio",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::off(),
        ).expect("should succeed");

        // For fold 0: train 7 bars (days 1–7), test 3 bars (days 8–10).
        let f0 = &result.folds[0];
        let train_days = (f0.train_range.end - f0.train_range.start).num_days() as usize + 1;
        let test_days  = (f0.test_range.end  - f0.test_range.start).num_days()  as usize + 1;
        assert_eq!(train_days, 7, "expected 7 train bars, got {train_days}");
        assert_eq!(test_days,  3, "expected 3 test bars, got {test_days}");
    }

    #[test]
    fn too_few_candles_returns_error() {
        let candles = synthetic_candles(3); // not enough for n_windows=2
        let grid = simple_grid();
        let result = run_walk_forward(
            &candles, &grid, 5, 0.7, "sharpe_ratio",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::off(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn invalid_n_windows_returns_error() {
        let candles = synthetic_candles(30);
        let grid = simple_grid();
        let result = run_walk_forward(
            &candles, &grid, 1, 0.7, "sharpe_ratio",
            10_000.0, ExecutionCosts::default(), FillTiming::Close,
            &BootstrapConfig::off(),
        );
        assert!(result.is_err());
    }
}
