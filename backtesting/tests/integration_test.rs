use std::collections::VecDeque;
use backtesting::data::{CSVDataSource, DataSource};
use backtesting::engine::{BacktestEngine, FillTiming};
use backtesting::portfolio::Portfolio;
use backtesting::strategy::{Signal, Strategy};
use backtesting::strategy::ma::{MAType, MovingAverageCrossover};
use backtesting::strategy::rsi::RSI;
use backtesting::portfolio::ExecutionCosts;
use backtesting::portfolio_backtest::{run_portfolio, PortfolioAsset};
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
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::Close);
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
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::Close);
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
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::Close);
    engine.run(&candles);
    let result = engine.result();

    assert!(result.metrics.total_return.is_finite());
    assert!(result.metrics.max_drawdown.is_finite());
    assert!(result.metrics.sharpe_ratio.is_finite());
}

/// Legacy assertion: under Close timing, buy-and-hold fills at candles[0].close.
#[test]
fn buy_and_hold_baseline_close_timing() {
    let candles = load_fixture("synthetic_30.csv");
    let buy_price = candles[0].close;  // 100.0
    let last_price = candles[candles.len() - 1].close;  // 100.0
    let initial_cash = 10_000.0_f64;
    let shares = (initial_cash / buy_price) as u32;
    let expected_nav = shares as f64 * last_price;

    let strategy = BuyOnceStrategy::new();
    let portfolio = Portfolio::new(initial_cash, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::Close);
    engine.run(&candles);
    let result = engine.result();

    let actual_nav = result.equity_curve.last().unwrap().nav;
    let diff = (actual_nav - expected_nav).abs() / expected_nav;
    assert!(diff < 0.01, "buy-and-hold NAV {actual_nav:.2} differs from expected {expected_nav:.2} by {:.2}%", diff * 100.0);
}

/// Under NextOpen timing, buy-and-hold fills at candles[1].open, not candles[0].close.
#[test]
fn buy_and_hold_baseline_next_open_timing() {
    let candles = load_fixture("synthetic_30.csv");
    // Fill happens at candles[1].open (bar 1 opens after bar 0's signal).
    let buy_price = candles[1].open;
    let last_price = candles[candles.len() - 1].close;
    let initial_cash = 10_000.0_f64;
    let shares = (initial_cash / buy_price) as u32;
    let expected_nav = shares as f64 * last_price + (initial_cash - shares as f64 * buy_price);

    let strategy = BuyOnceStrategy::new();
    let portfolio = Portfolio::new(initial_cash, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::NextOpen);
    engine.run(&candles);
    let result = engine.result();

    assert_eq!(result.trades.len(), 1, "should have exactly one buy trade");
    assert!((result.trades[0].price - buy_price).abs() < 1e-9,
        "NextOpen fill price {:.4} should equal candles[1].open {buy_price:.4}", result.trades[0].price);
    let actual_nav = result.equity_curve.last().unwrap().nav;
    let diff = (actual_nav - expected_nav).abs();
    assert!(diff < 1.0, "NextOpen NAV {actual_nav:.2} vs expected {expected_nav:.2}, diff={diff:.6}");
}

/// A signal emitted on the final bar under NextOpen must not produce a fill.
#[test]
fn next_open_last_bar_signal_is_not_filled() {
    let candles = load_fixture("synthetic_30.csv");
    let n = candles.len();

    // Send Hold for all bars except the last, then Buy on the last bar.
    let mut signals: Vec<Signal> = vec![Signal::Hold; n - 1];
    signals.push(Signal::Buy);

    let strategy = MockStrategy::new(signals);
    let portfolio = Portfolio::new(10_000.0, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::NextOpen);
    engine.run(&candles);
    let result = engine.result();

    assert!(result.trades.is_empty(), "last-bar signal must not produce a fill under NextOpen");
}

#[test]
fn zero_initial_cash_produces_no_trades() {
    let candles = load_fixture("synthetic_30.csv");
    let strategy = MovingAverageCrossover::new(MAType::SMA, 3, 5);
    let portfolio = Portfolio::new(0.0, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::Close);
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
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::Close);
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
fn portfolio_of_two_assets_aggregates_curve_and_breakdown() {
    let candles = load_fixture("synthetic_30.csv");
    let assets = vec![
        PortfolioAsset {
            symbol: "MA".to_string(),
            weight: 1.0,
            candles: candles.clone(),
            strategy: Box::new(MovingAverageCrossover::new(MAType::SMA, 3, 5)),
        },
        PortfolioAsset {
            symbol: "RSI".to_string(),
            weight: 1.0,
            candles: candles.clone(),
            strategy: Box::new(RSI::new(7)),
        },
    ];
    let res = run_portfolio(assets, 10_000.0, ExecutionCosts::default(), None, FillTiming::Close);

    // Both assets share the same 30 fixture dates → aggregate curve has 30 points.
    assert_eq!(res.equity_curve.len(), 30);
    assert_eq!(res.assets.len(), 2);
    // Equal weight splits the 10k capital evenly.
    assert!((res.assets[0].allocated_cash - 5_000.0).abs() < 1e-9);
    assert!((res.assets[1].allocated_cash - 5_000.0).abs() < 1e-9);
    // Portfolio metrics must be finite.
    assert!(res.metrics.total_return.is_finite());
    assert!(res.metrics.max_drawdown.is_finite());
    assert!(res.benchmark.total_return.is_finite());
    // Aggregate NAV at the final date equals the sum of per-asset final NAVs.
    let agg_last = res.equity_curve.last().unwrap().nav;
    let sum_last: f64 = res.assets.iter().map(|a| a.result.equity_curve.last().unwrap().nav).sum();
    assert!((agg_last - sum_last).abs() < 1e-6);
}

#[test]
fn date_range_in_equity_curve_matches_fixture() {
    let candles = load_fixture("synthetic_30.csv");
    let strategy = MockStrategy::new(vec![]);
    let portfolio = Portfolio::new(10_000.0, "SYNTH".to_string());
    let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::Close);
    engine.run(&candles);
    let result = engine.result();

    let expected_first = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let expected_last  = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
    assert_eq!(result.equity_curve.first().unwrap().date, expected_first);
    assert_eq!(result.equity_curve.last().unwrap().date, expected_last);
}
