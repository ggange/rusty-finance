use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde::Deserialize;
use backtesting::{
    data::{Candle, CSVDataSource, DataSource},
    engine::BacktestEngine,
    portfolio::{ExecutionCosts, Portfolio},
    strategy::ma::{MAType, MovingAverageCrossover},
    strategy::rsi::RSI,
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
#[pyo3(signature = (strategy_json, candles_json, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run(
    strategy_json: &str,
    candles_json: &str,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
) -> PyResult<String> {
    let spec: StrategySpec = serde_json::from_str(strategy_json)
        .map_err(|e| PyValueError::new_err(format!("invalid strategy JSON: {e}")))?;
    let candles = parse_candles(candles_json)?;

    match spec {
        StrategySpec::MaEma { short_window, long_window } => {
            validate_ma_windows(short_window, long_window)?;
            dispatch(MovingAverageCrossover::new(MAType::EMA, short_window, long_window),
                initial_cash, commission, slippage_pct, &candles)
        }
        StrategySpec::MaSma { short_window, long_window } => {
            validate_ma_windows(short_window, long_window)?;
            dispatch(MovingAverageCrossover::new(MAType::SMA, short_window, long_window),
                initial_cash, commission, slippage_pct, &candles)
        }
        StrategySpec::MaWma { short_window, long_window } => {
            validate_ma_windows(short_window, long_window)?;
            dispatch(MovingAverageCrossover::new(MAType::WMA, short_window, long_window),
                initial_cash, commission, slippage_pct, &candles)
        }
        StrategySpec::Rsi { period } => {
            if period < 2 {
                return Err(PyValueError::new_err("RSI period must be >= 2"));
            }
            dispatch(RSI::new(period), initial_cash, commission, slippage_pct, &candles)
        }
    }
}

// ─── Convenience CSV functions (used by the CLI binary) ──────────────────────

#[pyfunction]
#[pyo3(signature = (candles_json, short_window=5, long_window=20, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_ma(candles_json: &str, short_window: usize, long_window: usize,
          initial_cash: f64, commission: f64, slippage_pct: f64) -> PyResult<String> {
    validate_ma_windows(short_window, long_window)?;
    let candles = parse_candles(candles_json)?;
    dispatch(MovingAverageCrossover::new(MAType::EMA, short_window, long_window),
        initial_cash, commission, slippage_pct, &candles)
}

#[pyfunction]
#[pyo3(signature = (candles_json, period=14, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_rsi(candles_json: &str, period: usize, initial_cash: f64,
           commission: f64, slippage_pct: f64) -> PyResult<String> {
    let candles = parse_candles(candles_json)?;
    dispatch(RSI::new(period), initial_cash, commission, slippage_pct, &candles)
}

#[pyfunction]
#[pyo3(signature = (csv_path, short_window=5, long_window=20, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_ma_csv(csv_path: &str, short_window: usize, long_window: usize,
              initial_cash: f64, commission: f64, slippage_pct: f64) -> PyResult<String> {
    validate_ma_windows(short_window, long_window)?;
    let candles = load_csv(csv_path)?;
    dispatch(MovingAverageCrossover::new(MAType::EMA, short_window, long_window),
        initial_cash, commission, slippage_pct, &candles)
}

#[pyfunction]
#[pyo3(signature = (csv_path, period=14, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_rsi_csv(csv_path: &str, period: usize, initial_cash: f64,
               commission: f64, slippage_pct: f64) -> PyResult<String> {
    let candles = load_csv(csv_path)?;
    dispatch(RSI::new(period), initial_cash, commission, slippage_pct, &candles)
}

// ─── Internals ────────────────────────────────────────────────────────────────

fn dispatch<S: backtesting::strategy::Strategy>(
    strategy: S,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
    candles: &[Candle],
) -> PyResult<String> {
    let portfolio = Portfolio::new(initial_cash, "SYMBOL".to_string())
        .with_costs(ExecutionCosts { commission_per_trade: commission, slippage_pct });
    let mut engine = BacktestEngine::new(strategy, portfolio);
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
    m.add_function(wrap_pyfunction!(run_ma, m)?)?;
    m.add_function(wrap_pyfunction!(run_rsi, m)?)?;
    m.add_function(wrap_pyfunction!(run_ma_csv, m)?)?;
    m.add_function(wrap_pyfunction!(run_rsi_csv, m)?)?;
    Ok(())
}
