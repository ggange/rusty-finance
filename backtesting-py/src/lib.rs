use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde::Deserialize;
use backtesting::{
    data::{Candle, CSVDataSource, DataSource},
    engine::BacktestEngine,
    portfolio::{ExecutionCosts, Portfolio},
    portfolio_backtest::{run_portfolio as run_portfolio_core, PortfolioAsset},
    strategy::ma::{MAType, MovingAverageCrossover},
    strategy::rsi::RSI,
    strategy::macd::MACD,
    strategy::bollinger_bands::BollingerBands as BBands,
    strategy::Strategy,
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
    let strategy = build_strategy(spec)?;
    dispatch(strategy, initial_cash, commission, slippage_pct, &candles)
}

/// Run a multi-asset portfolio backtest.
///
/// `portfolio_json` is a JSON array of assets, each:
/// `{"symbol": "AAPL", "weight": 0.5, "strategy": {"type": "ma_ema", ...},
///   "candles": [ ... ]}`. `weight` is optional (defaults to equal weighting).
///
/// Returns a JSON string with `equity_curve`, `metrics`, `benchmark`, and a
/// per-asset `assets` breakdown.
#[pyfunction]
#[pyo3(signature = (portfolio_json, initial_cash=10_000.0, commission=0.0, slippage_pct=0.0))]
fn run_portfolio(
    portfolio_json: &str,
    initial_cash: f64,
    commission: f64,
    slippage_pct: f64,
) -> PyResult<String> {
    let specs: Vec<AssetIn> = serde_json::from_str(portfolio_json)
        .map_err(|e| PyValueError::new_err(format!("invalid portfolio JSON: {e}")))?;
    if specs.is_empty() {
        return Err(PyValueError::new_err("portfolio must contain at least one asset"));
    }

    let mut assets: Vec<PortfolioAsset> = Vec::with_capacity(specs.len());
    for spec in specs {
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
    let result = run_portfolio_core(assets, initial_cash, costs);
    to_json(result)
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
    m.add_function(wrap_pyfunction!(run_portfolio, m)?)?;
    m.add_function(wrap_pyfunction!(run_ma, m)?)?;
    m.add_function(wrap_pyfunction!(run_rsi, m)?)?;
    m.add_function(wrap_pyfunction!(run_ma_csv, m)?)?;
    m.add_function(wrap_pyfunction!(run_rsi_csv, m)?)?;
    Ok(())
}
