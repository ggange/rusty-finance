use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use backtesting::{
    data::{Candle, CSVDataSource, DataSource},
    engine::BacktestEngine,
    portfolio::{ExecutionCosts, Portfolio, SizingRule},
    strategy::ma::{MAType, MovingAverageCrossover},
    strategy::rsi::RSI,
};

// ─── Public Python functions ──────────────────────────────────────────────────

/// Run a Moving Average EMA crossover backtest on candle data provided as a
/// JSON array. Returns a JSON string containing `equity_curve`, `trades`,
/// `metrics`, and `benchmark`.
///
/// Each candle object must have keys: `date`, `open`, `high`, `low`, `close`, `volume`.
#[pyfunction]
#[pyo3(signature = (candles_json, short_window=5, long_window=20, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_ma(
    candles_json: &str,
    short_window: usize,
    long_window: usize,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
) -> PyResult<String> {
    let candles = parse_candles(candles_json)?;
    run_engine(
        MovingAverageCrossover::new(MAType::EMA, short_window, long_window),
        initial_cash, commission, slippage_pct, &candles,
    )
}

/// Run an RSI backtest on candle data provided as a JSON array.
#[pyfunction]
#[pyo3(signature = (candles_json, period=14, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_rsi(
    candles_json: &str,
    period: usize,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
) -> PyResult<String> {
    let candles = parse_candles(candles_json)?;
    run_engine(RSI::new(period), initial_cash, commission, slippage_pct, &candles)
}

/// Convenience: run MA backtest loading candles from a CSV file path.
#[pyfunction]
#[pyo3(signature = (csv_path, short_window=5, long_window=20, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_ma_csv(
    csv_path: &str,
    short_window: usize,
    long_window: usize,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
) -> PyResult<String> {
    let candles = load_csv(csv_path)?;
    run_engine(
        MovingAverageCrossover::new(MAType::EMA, short_window, long_window),
        initial_cash, commission, slippage_pct, &candles,
    )
}

/// Convenience: run RSI backtest loading candles from a CSV file path.
#[pyfunction]
#[pyo3(signature = (csv_path, period=14, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_rsi_csv(
    csv_path: &str,
    period: usize,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
) -> PyResult<String> {
    let candles = load_csv(csv_path)?;
    run_engine(RSI::new(period), initial_cash, commission, slippage_pct, &candles)
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn run_engine<S: backtesting::strategy::Strategy>(
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
    m.add_function(wrap_pyfunction!(run_ma, m)?)?;
    m.add_function(wrap_pyfunction!(run_rsi, m)?)?;
    m.add_function(wrap_pyfunction!(run_ma_csv, m)?)?;
    m.add_function(wrap_pyfunction!(run_rsi_csv, m)?)?;
    Ok(())
}
