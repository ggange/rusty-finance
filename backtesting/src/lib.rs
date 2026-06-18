//! # backtesting
//!
//! A Rust library for running trading strategy backtests against historical OHLCV data.
//!
//! ## Architecture
//!
//! 1. Load [`data::Candle`] bars from a [`data::CSVDataSource`].
//! 2. Implement [`strategy::Strategy`] (or use the built-ins in [`strategy::ma`] / [`strategy::rsi`]).
//! 3. Feed candles to a [`engine::BacktestEngine`] and call [`engine::BacktestEngine::result`].
//! 4. Inspect [`engine::BacktestResult`] for the equity curve, trade log, and [`metrics::Metrics`].
//!
//! ## Quick Start
//!
//! ```no_run
//! use backtesting::data::{CSVDataSource, DataSource};
//! use backtesting::engine::BacktestEngine;
//! use backtesting::portfolio::Portfolio;
//! use backtesting::strategy::ma::{MAType, MovingAverageCrossover};
//!
//! let candles = CSVDataSource { file_path: "prices.csv".to_string() }
//!     .load()
//!     .expect("failed to load CSV");
//!
//! let strategy = MovingAverageCrossover::new(MAType::EMA, 5, 20);
//! let portfolio = Portfolio::new(10_000.0, "AAPL".to_string());
//! let mut engine = BacktestEngine::new(strategy, portfolio);
//! engine.run(&candles);
//! let result = engine.result();
//! println!("total return: {:.2}%", result.metrics.total_return * 100.0);
//! ```

pub mod data;
pub mod strategy;
pub mod engine;
pub mod portfolio;
pub mod portfolio_backtest;
pub mod metrics;
