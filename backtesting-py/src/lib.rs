use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use backtesting::{
    data::{CSVDataSource, DataSource},
    engine::BacktestEngine,
    portfolio::Portfolio,
    strategy::ma::{MAType, MovingAverageCrossover},
    strategy::rsi::RSI,
};

/// Run a Moving Average crossover backtest. Returns a JSON string with
/// equity_curve, trades, and metrics (total_return, max_drawdown, sharpe_ratio).
#[pyfunction]
#[pyo3(signature = (csv_path, short_window=5, long_window=20, initial_cash=10_000.0))]
fn run_ma(
    csv_path: &str,
    short_window: usize,
    long_window: usize,
    initial_cash: f64,
) -> PyResult<String> {
    let candles = load_csv(csv_path)?;
    let mut engine = BacktestEngine::new(
        MovingAverageCrossover::new(MAType::EMA, short_window, long_window),
        Portfolio::new(initial_cash, "SYMBOL".to_string()),
    );
    engine.run(&candles);
    to_json(engine.result())
}

/// Run an RSI backtest. Returns a JSON string with equity_curve, trades, and metrics.
#[pyfunction]
#[pyo3(signature = (csv_path, period=14, initial_cash=10_000.0))]
fn run_rsi(csv_path: &str, period: usize, initial_cash: f64) -> PyResult<String> {
    let candles = load_csv(csv_path)?;
    let mut engine = BacktestEngine::new(
        RSI::new(period),
        Portfolio::new(initial_cash, "SYMBOL".to_string()),
    );
    engine.run(&candles);
    to_json(engine.result())
}

fn load_csv(path: &str) -> PyResult<Vec<backtesting::data::Candle>> {
    CSVDataSource { file_path: path.to_string() }
        .load()
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

fn to_json<T: serde::Serialize>(v: T) -> PyResult<String> {
    serde_json::to_string(&v).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

#[pymodule]
fn backtesting_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_ma, m)?)?;
    m.add_function(wrap_pyfunction!(run_rsi, m)?)?;
    Ok(())
}
