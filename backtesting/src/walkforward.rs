//! Walk-forward validation: for each rolling fold, train on the first
//! `train_frac` of the fold and test on the remainder. The best parameter
//! combination (by `metric`) from the train slice is then run on the test slice.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::data::Candle;
use crate::engine::{BacktestEngine, FillTiming};
use crate::metrics::Metrics;
use crate::portfolio::{ExecutionCosts, Portfolio};
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
    /// Metrics from the same combo run on the test slice.
    pub test_metrics: Metrics,
}

/// Output of a full walk-forward run.
#[derive(Debug, Serialize)]
pub struct WalkForwardResult {
    pub folds: Vec<WalkForwardFold>,
}

/// Run walk-forward validation.
///
/// Splits `candles` into `n_windows` rolling folds of equal length. Each fold
/// uses the first `train_frac` of its bars as the train slice and the rest as
/// the test slice. For each fold the full `strategy_grid` is evaluated on the
/// train slice; the combo with the best `metric` score is re-run on the test
/// slice and both metric sets are returned.
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

        // Run the entire grid on the train slice.
        let mut best_idx = 0usize;
        let mut best_score = f64::NEG_INFINITY;
        let mut train_metrics_vec: Vec<Metrics> = Vec::with_capacity(strategy_grid.len());

        for (gi, spec_val) in strategy_grid.iter().enumerate() {
            let m = run_one(spec_val, train, initial_cash, &costs, fill_timing)?;
            let score = metric_score(&m, metric);
            if score > best_score {
                best_score = score;
                best_idx = gi;
            }
            train_metrics_vec.push(m);
        }

        // Re-run the best combo on the test slice.
        let best_spec = &strategy_grid[best_idx];
        let test_metrics = run_one(best_spec, test, initial_cash, &costs, fill_timing)?;
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
        });
    }

    Ok(WalkForwardResult { folds })
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
        );
        assert!(result.is_err());
    }
}
