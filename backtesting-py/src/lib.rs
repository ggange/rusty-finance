use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use backtesting::{
    data::{Candle, CSVDataSource, DataSource},
    engine::{BacktestEngine, FillTiming},
    metrics::Metrics,
    portfolio::{ExecutionCosts, Portfolio},
    optimize::{optimize_weights as optimize_weights_core, OptimizerConfig},
    portfolio_backtest::{
        run_portfolio_with_policy, PortfolioAsset, RebalanceConfig, WeightPolicy,
    },
    strategy::ma::{MAType, MovingAverageCrossover},
    strategy::rsi::RSI,
    strategy::macd::MACD,
    strategy::bollinger_bands::BollingerBands as BBands,
    strategy::{Strategy, Signal},
};

// ─── Strategy registry ────────────────────────────────────────────────────────

/// Serde-tagged enum: the `type` field selects the strategy variant.
/// Matches the discriminator used in the Python API and OpenAPI schema.
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

/// One asset in a portfolio request. Candles are always inline here — the
/// Python layer resolves any named dataset to candles before calling in.
#[derive(Deserialize)]
struct AssetIn {
    symbol: String,
    #[serde(default)]
    weight: Option<f64>,
    strategy: StrategySpec,
    candles: Vec<Candle>,
}

/// Top-level portfolio request envelope sent from Python.
#[derive(Deserialize)]
struct PortfolioRequestIn {
    assets: Vec<AssetIn>,
    #[serde(default)]
    rebalance: Option<RebalanceConfig>,
    /// How target weights are decided. Absent means the manual weights.
    #[serde(default)]
    weight_policy: Option<WeightPolicy>,
}

/// Build a boxed strategy from a spec, validating its parameters.
fn build_strategy(spec: StrategySpec) -> PyResult<Box<dyn Strategy>> {
    Ok(match spec {
        StrategySpec::MaEma { short_window, long_window } => {
            validate_ma_windows(short_window, long_window)?;
            Box::new(MovingAverageCrossover::new(MAType::EMA, short_window, long_window))
        }
        StrategySpec::MaSma { short_window, long_window } => {
            validate_ma_windows(short_window, long_window)?;
            Box::new(MovingAverageCrossover::new(MAType::SMA, short_window, long_window))
        }
        StrategySpec::MaWma { short_window, long_window } => {
            validate_ma_windows(short_window, long_window)?;
            Box::new(MovingAverageCrossover::new(MAType::WMA, short_window, long_window))
        }
        StrategySpec::Rsi { period } => {
            if period < 2 {
                return Err(PyValueError::new_err("RSI period must be >= 2"));
            }
            Box::new(RSI::new(period))
        }
        StrategySpec::Macd { fast_period, slow_period, signal_period } => {
            if fast_period < 2 || slow_period < 2 || signal_period < 2 {
                return Err(PyValueError::new_err("MACD periods must be >= 2"));
            }
            if fast_period >= slow_period {
                return Err(PyValueError::new_err(
                    format!("fast_period ({fast_period}) must be < slow_period ({slow_period})")
                ));
            }
            Box::new(MACD::new(fast_period, slow_period, signal_period))
        }
        StrategySpec::BollingerBands { period, std_dev_mult } => {
            if period < 2 {
                return Err(PyValueError::new_err("Bollinger Bands period must be >= 2"));
            }
            if std_dev_mult <= 0.0 {
                return Err(PyValueError::new_err("std_dev_mult must be > 0"));
            }
            Box::new(BBands::new(period, std_dev_mult))
        }
    })
}

/// Parse a fill_timing string into the Rust enum. Defaults to NextOpen.
fn parse_fill_timing(s: &str) -> PyResult<FillTiming> {
    match s {
        "close" => Ok(FillTiming::Close),
        "next_open" => Ok(FillTiming::NextOpen),
        other => Err(PyValueError::new_err(format!(
            "unknown fill_timing {:?}: expected \"close\" or \"next_open\"", other
        ))),
    }
}

// ─── Primary API ──────────────────────────────────────────────────────────────

/// Run a backtest. `strategy_json` is a tagged object like
/// `{"type": "ma_ema", "short_window": 5, "long_window": 20}`.
///
/// `candles_json` is a JSON array of objects with keys:
/// `date`, `open`, `high`, `low`, `close`, `volume`.
///
/// Returns a JSON string with `equity_curve`, `trades`, `metrics`, `benchmark`.
#[pyfunction]
#[pyo3(signature = (strategy_json, candles_json, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0, fill_timing="next_open"))]
fn run(
    strategy_json: &str,
    candles_json: &str,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
    fill_timing: &str,
) -> PyResult<String> {
    let spec: StrategySpec = serde_json::from_str(strategy_json)
        .map_err(|e| PyValueError::new_err(format!("invalid strategy JSON: {e}")))?;
    let candles = parse_candles(candles_json)?;
    let timing = parse_fill_timing(fill_timing)?;
    let strategy = build_strategy(spec)?;
    dispatch(strategy, initial_cash, commission, slippage_pct, timing, &candles)
}

/// Run a multi-asset portfolio backtest.
///
/// `portfolio_json` is a JSON object:
/// `{"assets": [...], "rebalance": {"frequency": {"kind": "monthly"}}}`
/// where each asset is `{"symbol": "AAPL", "weight": 0.5, "strategy": {...}, "candles": [...]}`.
/// `weight` and `rebalance` are optional.
///
/// Returns a JSON string with `equity_curve`, `metrics`, `benchmark`, `risk`,
/// per-asset `assets` breakdown, and optional `rebalance_dates`.
#[pyfunction]
#[pyo3(signature = (portfolio_json, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0, fill_timing="next_open"))]
fn run_portfolio(
    portfolio_json: &str,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
    fill_timing: &str,
) -> PyResult<String> {
    let req: PortfolioRequestIn = serde_json::from_str(portfolio_json)
        .map_err(|e| PyValueError::new_err(format!("invalid portfolio JSON: {e}")))?;
    if req.assets.is_empty() {
        return Err(PyValueError::new_err("portfolio must contain at least one asset"));
    }

    let timing = parse_fill_timing(fill_timing)?;
    let mut assets: Vec<PortfolioAsset> = Vec::with_capacity(req.assets.len());
    for spec in req.assets {
        let weight = spec.weight.unwrap_or(1.0);
        let strategy = build_strategy(spec.strategy)?;
        assets.push(PortfolioAsset {
            symbol: spec.symbol,
            weight,
            candles: spec.candles,
            strategy,
        });
    }

    let costs = ExecutionCosts { commission_per_trade: commission, slippage_pct };
    let result = run_portfolio_with_policy(
        assets,
        initial_cash,
        costs,
        req.rebalance,
        timing,
        req.weight_policy.unwrap_or_default(),
    );
    to_json(result)
}

/// Solve portfolio weights directly, without running a backtest.
///
/// `returns_json` is a JSON array of per-asset daily return series (asset-major).
/// `config_json` is an [`OptimizerConfig`]:
/// `{"objective": "risk_parity", "shrinkage": 0.2, "max_weight": null, "max_iter": 500, "tol": 1e-9}`.
///
/// Returns `{weights, expected_volatility, expected_return, risk_contribution,
/// iterations, hit_iteration_limit}`.
#[pyfunction]
#[pyo3(signature = (returns_json, config_json))]
fn optimize_weights(returns_json: &str, config_json: &str) -> PyResult<String> {
    let returns: Vec<Vec<f64>> = serde_json::from_str(returns_json)
        .map_err(|e| PyValueError::new_err(format!("invalid returns JSON: {e}")))?;
    let config: OptimizerConfig = serde_json::from_str(config_json)
        .map_err(|e| PyValueError::new_err(format!("invalid optimizer config: {e}")))?;
    if !(0.0..=1.0).contains(&config.shrinkage) {
        return Err(PyValueError::new_err("shrinkage must be between 0 and 1"));
    }
    if let Some(cap) = config.max_weight {
        if cap <= 0.0 || cap > 1.0 {
            return Err(PyValueError::new_err("max_weight must be in (0, 1]"));
        }
    }
    to_json(optimize_weights_core(&returns, &config))
}

/// Run the same strategy with different parameter sets over one candle series.
///
/// `strategy_grid_json` is a JSON array of full strategy objects (including `type`),
/// each representing one parameter combination to test.
///
/// Returns a JSON array of `{ params, metrics }` objects in the same order as the grid.
#[pyfunction]
#[pyo3(signature = (strategy_grid_json, candles_json, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0, fill_timing="next_open"))]
fn run_sweep(
    strategy_grid_json: &str,
    candles_json: &str,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
    fill_timing: &str,
) -> PyResult<String> {
    #[derive(Serialize)]
    struct SweepPoint {
        params: HashMap<String, serde_json::Value>,
        metrics: Metrics,
    }

    let timing = parse_fill_timing(fill_timing)?;
    let candles = parse_candles(candles_json)?;
    let raw_specs: Vec<serde_json::Value> = serde_json::from_str(strategy_grid_json)
        .map_err(|e| PyValueError::new_err(format!("invalid strategy_grid JSON: {e}")))?;

    let mut results: Vec<SweepPoint> = Vec::with_capacity(raw_specs.len());
    for raw in raw_specs {
        let spec: StrategySpec = serde_json::from_value(raw.clone())
            .map_err(|e| PyValueError::new_err(format!("invalid strategy spec: {e}")))?;
        let params: HashMap<String, serde_json::Value> = raw
            .as_object()
            .map(|obj| obj.iter().filter(|(k, _)| k.as_str() != "type").map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        let strategy = build_strategy(spec)?;
        let portfolio = Portfolio::new(initial_cash, "SWEEP".to_string())
            .with_costs(ExecutionCosts { commission_per_trade: commission, slippage_pct });
        let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(timing);
        engine.run(&candles);
        results.push(SweepPoint { params, metrics: engine.result().metrics });
    }
    to_json(results)
}

/// Run walk-forward validation over a candle dataset.
///
/// `strategy_grid_json` is a JSON array of strategy spec objects (same format
/// as `run_sweep`). The candles are split into `n_windows` rolling folds; for
/// each fold the best parameter combo (by `metric`) is selected on the train
/// slice and evaluated on the test slice.
///
/// Returns a JSON string `{ "folds": [...] }`.
#[pyfunction]
#[pyo3(signature = (strategy_grid_json, candles_json, n_windows=5, train_frac=0.7, metric="sharpe_ratio", initial_cash=10_000.0, commission=0.0, slippage_pct=0.0, fill_timing="next_open"))]
fn run_walk_forward(
    strategy_grid_json: &str,
    candles_json: &str,
    n_windows: usize,
    train_frac: f64,
    metric: &str,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
    fill_timing: &str,
) -> PyResult<String> {
    use backtesting::walkforward;
    let candles = parse_candles(candles_json)?;
    let grid: Vec<serde_json::Value> = serde_json::from_str(strategy_grid_json)
        .map_err(|e| PyValueError::new_err(format!("invalid strategy_grid JSON: {e}")))?;
    let timing = parse_fill_timing(fill_timing)?;
    let costs = ExecutionCosts { commission_per_trade: commission, slippage_pct };
    let result = walkforward::run_walk_forward(&candles, &grid, n_windows, train_frac, metric, initial_cash, costs, timing)
        .map_err(|e| PyValueError::new_err(e))?;
    to_json(result)
}

/// Return the signal produced by the strategy on the final bar of the candle series.
///
/// `strategy_json` — same tagged format as `run` (e.g. `{"type":"rsi","period":14}`).
/// `candles_json`  — JSON array of candle objects.
///
/// Returns `{"signal": "buy"|"sell"|"hold", "date": "YYYY-MM-DD", "close": <f64>, "bars": <usize>}`.
#[pyfunction]
fn latest_signal(strategy_json: &str, candles_json: &str) -> PyResult<String> {
    #[derive(Serialize)]
    struct LatestSignalResult {
        signal: String,
        date: String,
        close: f64,
        bars: usize,
    }

    let spec: StrategySpec = serde_json::from_str(strategy_json)
        .map_err(|e| PyValueError::new_err(format!("invalid strategy JSON: {e}")))?;
    let candles = parse_candles(candles_json)?;
    if candles.is_empty() {
        return Err(PyValueError::new_err("candles must not be empty"));
    }
    let mut strategy = build_strategy(spec)?;
    let mut last_signal = Signal::Hold;
    for candle in &candles {
        last_signal = strategy.on_bar(candle);
    }
    let last = candles.last().unwrap();
    let signal_str = match last_signal {
        Signal::Buy  => "buy",
        Signal::Sell => "sell",
        Signal::Hold => "hold",
    };
    to_json(LatestSignalResult {
        signal: signal_str.to_string(),
        date: last.date.to_string(),
        close: last.close,
        bars: candles.len(),
    })
}

// ─── Convenience CSV functions (used by the CLI binary) ──────────────────────

#[pyfunction]
#[pyo3(signature = (candles_json, short_window=5, long_window=20, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_ma(candles_json: &str, short_window: usize, long_window: usize,
          initial_cash: f64, commission: f64, slippage_pct: f64) -> PyResult<String> {
    validate_ma_windows(short_window, long_window)?;
    let candles = parse_candles(candles_json)?;
    dispatch(MovingAverageCrossover::new(MAType::EMA, short_window, long_window),
        initial_cash, commission, slippage_pct, FillTiming::NextOpen, &candles)
}

#[pyfunction]
#[pyo3(signature = (candles_json, period=14, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_rsi(candles_json: &str, period: usize, initial_cash: f64,
           commission: f64, slippage_pct: f64) -> PyResult<String> {
    let candles = parse_candles(candles_json)?;
    dispatch(RSI::new(period), initial_cash, commission, slippage_pct, FillTiming::NextOpen, &candles)
}

#[pyfunction]
#[pyo3(signature = (csv_path, short_window=5, long_window=20, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_ma_csv(csv_path: &str, short_window: usize, long_window: usize,
              initial_cash: f64, commission: f64, slippage_pct: f64) -> PyResult<String> {
    validate_ma_windows(short_window, long_window)?;
    let candles = load_csv(csv_path)?;
    dispatch(MovingAverageCrossover::new(MAType::EMA, short_window, long_window),
        initial_cash, commission, slippage_pct, FillTiming::NextOpen, &candles)
}

#[pyfunction]
#[pyo3(signature = (csv_path, period=14, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_rsi_csv(csv_path: &str, period: usize, initial_cash: f64,
               commission: f64, slippage_pct: f64) -> PyResult<String> {
    let candles = load_csv(csv_path)?;
    dispatch(RSI::new(period), initial_cash, commission, slippage_pct, FillTiming::NextOpen, &candles)
}

// ─── Internals ────────────────────────────────────────────────────────────────

fn dispatch<S: backtesting::strategy::Strategy>(
    strategy: S,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
    fill_timing: FillTiming,
    candles: &[Candle],
) -> PyResult<String> {
    let portfolio = Portfolio::new(initial_cash, "SYMBOL".to_string())
        .with_costs(ExecutionCosts { commission_per_trade: commission, slippage_pct });
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(fill_timing);
    engine.run(candles);
    to_json(engine.result())
}

fn validate_ma_windows(short: usize, long: usize) -> PyResult<()> {
    if short == 0 || long == 0 {
        return Err(PyValueError::new_err("MA windows must be > 0"));
    }
    if short >= long {
        return Err(PyValueError::new_err(
            format!("short_window ({short}) must be less than long_window ({long})")
        ));
    }
    Ok(())
}

fn parse_candles(json: &str) -> PyResult<Vec<Candle>> {
    serde_json::from_str(json)
        .map_err(|e| PyRuntimeError::new_err(format!("invalid candles JSON: {e}")))
}

fn load_csv(path: &str) -> PyResult<Vec<Candle>> {
    CSVDataSource { file_path: path.to_string() }
        .load()
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

fn to_json<T: serde::Serialize>(v: T) -> PyResult<String> {
    serde_json::to_string(&v).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

// ─── Module ───────────────────────────────────────────────────────────────────

#[pymodule]
fn backtesting_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run, m)?)?;
    m.add_function(wrap_pyfunction!(run_portfolio, m)?)?;
    m.add_function(wrap_pyfunction!(optimize_weights, m)?)?;
    m.add_function(wrap_pyfunction!(run_sweep, m)?)?;
    m.add_function(wrap_pyfunction!(run_walk_forward, m)?)?;
    m.add_function(wrap_pyfunction!(latest_signal, m)?)?;
    m.add_function(wrap_pyfunction!(run_ma, m)?)?;
    m.add_function(wrap_pyfunction!(run_rsi, m)?)?;
    m.add_function(wrap_pyfunction!(run_ma_csv, m)?)?;
    m.add_function(wrap_pyfunction!(run_rsi_csv, m)?)?;
    Ok(())
}
