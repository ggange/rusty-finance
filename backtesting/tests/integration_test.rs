use std::collections::VecDeque;
use backtesting::data::{CSVDataSource, DataSource};
use backtesting::engine::BacktestEngine;
use backtesting::portfolio::Portfolio;
use backtesting::strategy::{Signal, Strategy};
use backtesting::strategy::ma::{MAType, MovingAverageCrossover};
use backtesting::strategy::rsi::RSI;
use backtesting::data::Candle;
use chrono::NaiveDate;

fn fixture(name: &str) -> String {
    format!("{}/../data/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
}

fn load_fixture(name: &str) -> Vec<Candle> {
    CSVDataSource { file_path: fixture(name) }
        .load()
        .expect("fixture should load")
}

struct BuyOnceStrategy {
    bought: bool,
}

impl BuyOnceStrategy {
    fn new() -> Self { Self { bought: false } }
}

impl Strategy for BuyOnceStrategy {
    fn on_bar(&mut self, _candle: &Candle) -> Signal {
        if !self.bought {
            self.bought = true;
            Signal::Buy
        } else {
            Signal::Hold
        }
    }
}

struct MockStrategy {
    signals: VecDeque<Signal>,
}

impl MockStrategy {
    fn new(signals: Vec<Signal>) -> Self {
        Self { signals: signals.into() }
    }
}

impl Strategy for MockStrategy {
    fn on_bar(&mut self, _candle: &Candle) -> Signal {
        self.signals.pop_front().unwrap_or(Signal::Hold)
    }
}

#[test]
fn full_ma_backtest_on_fixture_produces_trades() {
    let candles = load_fixture("synthetic_30.csv");
    let strategy = MovingAverageCrossover::new(MAType::SMA, 3, 5);
    let portfolio = Portfolio::new(10_000.0, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio);
    engine.run(&candles);
    let result = engine.result();

    assert_eq!(result.equity_curve.len(), 30);
    assert!(!result.trades.is_empty(), "SMA(3,5) crossover should produce at least one trade");
}

#[test]
fn full_rsi_backtest_on_fixture_produces_buy() {
    let candles = load_fixture("synthetic_30.csv");
    let strategy = RSI::new(7);
    let portfolio = Portfolio::new(10_000.0, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio);
    engine.run(&candles);
    let result = engine.result();

    let has_buy = result.trades.iter().any(|t| t.action == Signal::Buy);
    assert!(has_buy, "RSI(7) should fire a Buy during the sustained falling phase");
}

#[test]
fn full_ma_backtest_metrics_are_finite() {
    let candles = load_fixture("synthetic_30.csv");
    let strategy = MovingAverageCrossover::new(MAType::SMA, 3, 5);
    let portfolio = Portfolio::new(10_000.0, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio);
    engine.run(&candles);
    let result = engine.result();

    assert!(result.metrics.total_return.is_finite());
    assert!(result.metrics.max_drawdown.is_finite());
    assert!(result.metrics.sharpe_ratio.is_finite());
}

#[test]
fn buy_and_hold_baseline() {
    let candles = load_fixture("synthetic_30.csv");
    let buy_price = candles[0].close;  // 100.0
    let last_price = candles[candles.len() - 1].close;  // 100.0
    let initial_cash = 10_000.0_f64;
    let shares = (initial_cash / buy_price) as u32;
    let expected_nav = shares as f64 * last_price;

    let strategy = BuyOnceStrategy::new();
    let portfolio = Portfolio::new(initial_cash, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio);
    engine.run(&candles);
    let result = engine.result();

    let actual_nav = result.equity_curve.last().unwrap().nav;
    let diff = (actual_nav - expected_nav).abs() / expected_nav;
    assert!(diff < 0.01, "buy-and-hold NAV {actual_nav:.2} differs from expected {expected_nav:.2} by {:.2}%", diff * 100.0);
}

#[test]
fn zero_initial_cash_produces_no_trades() {
    let candles = load_fixture("synthetic_30.csv");
    let strategy = MovingAverageCrossover::new(MAType::SMA, 3, 5);
    let portfolio = Portfolio::new(0.0, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio);
    engine.run(&candles);
    let result = engine.result();

    assert!(result.trades.is_empty(), "no cash means no trades can be executed");
}

#[test]
fn sma_crossover_buy_precedes_sell() {
    // The fixture is designed so SMA(3,5) produces exactly 1 Buy then 1 Sell
    let candles = load_fixture("synthetic_30.csv");
    let strategy = MovingAverageCrossover::new(MAType::SMA, 3, 5);
    let portfolio = Portfolio::new(10_000.0, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio);
    engine.run(&candles);
    let result = engine.result();

    let trades = &result.trades;
    assert!(!trades.is_empty());
    assert_eq!(trades[0].action, Signal::Buy, "first trade should be a Buy");
    if trades.len() > 1 {
        assert_eq!(trades[1].action, Signal::Sell, "second trade should be a Sell");
        assert!(trades[0].date < trades[1].date, "Buy must precede Sell");
    }
}

#[test]
fn date_range_in_equity_curve_matches_fixture() {
    let candles = load_fixture("synthetic_30.csv");
    let strategy = MockStrategy::new(vec![]);
    let portfolio = Portfolio::new(10_000.0, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio);
    engine.run(&candles);
    let result = engine.result();

    let expected_first = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let expected_last  = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
    assert_eq!(result.equity_curve.first().unwrap().date, expected_first);
    assert_eq!(result.equity_curve.last().unwrap().date, expected_last);
}
