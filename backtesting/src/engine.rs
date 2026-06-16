use serde::Serialize;
use crate::data::Candle;
use crate::strategy::{Signal, Strategy};
use crate::portfolio::{EquityPoint, Portfolio, TradeRecord};
use crate::metrics::{Benchmark, Metrics};

/// The complete output of a backtest run.
#[derive(Debug, Serialize, Clone)]
pub struct BacktestResult {
    /// NAV recorded after every bar, in chronological order.
    pub equity_curve: Vec<EquityPoint>,
    /// Every executed trade in the order it occurred.
    pub trades: Vec<TradeRecord>,
    /// Performance statistics for this strategy.
    pub metrics: Metrics,
    /// Buy-and-hold benchmark over the same period (total_return and CAGR).
    pub benchmark: Benchmark,
}

/// Drives a backtest by feeding candles to a [`Strategy`] and executing signals
/// against a [`Portfolio`].
pub struct BacktestEngine<S: Strategy> {
    strategy: S,
    portfolio: Portfolio,
    first_price: Option<f64>,
    last_price: Option<f64>,
}

impl<S: Strategy> BacktestEngine<S> {
    pub fn new(strategy: S, portfolio: Portfolio) -> Self {
        Self { strategy, portfolio, first_price: None, last_price: None }
    }

    /// Feed candles in chronological order. May be called multiple times to
    /// stream data in chunks; the equity curve and trade log accumulate.
    pub fn run(&mut self, candles: &[Candle]) {
        for candle in candles {
            if self.first_price.is_none() { self.first_price = Some(candle.close); }
            self.last_price = Some(candle.close);

            let signal = self.strategy.on_bar(candle);
            match signal {
                Signal::Buy  => self.portfolio.execute_buy(candle.close, candle.date),
                Signal::Sell => self.portfolio.execute_sell(candle.close, candle.date),
                Signal::Hold => (),
            }
            self.portfolio.record_nav(candle.close, candle.date);
        }
    }

    /// Consume the engine and return the full result including metrics and benchmark.
    pub fn result(self) -> BacktestResult {
        let metrics = Metrics::compute(self.portfolio.equity_curve(), self.portfolio.trades());
        let benchmark = match (self.first_price, self.last_price) {
            (Some(first), Some(last)) => Benchmark::compute(
                self.portfolio.initial_cash, first, last,
                self.portfolio.equity_curve().len(),
            ),
            _ => Benchmark { total_return: 0.0, cagr: 0.0 },
        };
        BacktestResult {
            equity_curve: self.portfolio.equity_curve().to_vec(),
            trades: self.portfolio.trades().to_vec(),
            metrics,
            benchmark,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use chrono::NaiveDate;
    use crate::strategy::{Signal, Strategy};

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

    fn candle(close: f64, day: u32) -> Candle {
        Candle {
            date: NaiveDate::from_ymd_opt(2024, 1, day).unwrap(),
            open: close, high: close, low: close, close, volume: 0,
        }
    }

    fn run(signals: Vec<Signal>, closes: &[f64], cash: f64) -> BacktestResult {
        let strategy = MockStrategy::new(signals);
        let portfolio = Portfolio::new(cash, "TEST".to_string());
        let mut engine = BacktestEngine::new(strategy, portfolio);
        let candles: Vec<Candle> = closes.iter().enumerate()
            .map(|(i, &c)| candle(c, i as u32 + 1))
            .collect();
        engine.run(&candles);
        engine.result()
    }

    #[test]
    fn equity_curve_has_one_point_per_bar() {
        let result = run(vec![Signal::Hold; 5], &[100.0; 5], 1000.0);
        assert_eq!(result.equity_curve.len(), 5);
    }

    #[test]
    fn buy_signal_creates_trade_record() {
        let result = run(
            vec![Signal::Buy, Signal::Hold, Signal::Hold],
            &[100.0, 100.0, 100.0],
            1000.0,
        );
        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].action, Signal::Buy);
    }

    #[test]
    fn sell_signal_creates_trade_record() {
        let result = run(
            vec![Signal::Buy, Signal::Sell, Signal::Hold],
            &[100.0, 100.0, 100.0],
            1000.0,
        );
        assert_eq!(result.trades.len(), 2);
        assert_eq!(result.trades[1].action, Signal::Sell);
    }

    #[test]
    fn hold_signal_creates_no_trade() {
        let result = run(vec![Signal::Hold; 3], &[100.0; 3], 1000.0);
        assert!(result.trades.is_empty());
    }

    #[test]
    fn nav_reflects_price_appreciation() {
        let result = run(vec![Signal::Buy, Signal::Hold], &[100.0, 150.0], 1000.0);
        assert!((result.equity_curve[0].nav - 1000.0).abs() < 1e-9);
        assert!((result.equity_curve[1].nav - 1500.0).abs() < 1e-9);
    }

    #[test]
    fn empty_candles_returns_empty_result() {
        let result = run(vec![], &[], 1000.0);
        assert!(result.equity_curve.is_empty());
        assert!(result.trades.is_empty());
        assert_eq!(result.metrics.total_return, 0.0);
    }

    #[test]
    fn metrics_wired_from_equity_curve() {
        let result = run(vec![Signal::Buy, Signal::Hold], &[100.0, 200.0], 1000.0);
        assert!((result.metrics.total_return - 1.0).abs() < 1e-9);
    }

    #[test]
    fn benchmark_present_after_run() {
        let result = run(vec![Signal::Hold; 3], &[100.0, 110.0, 120.0], 1000.0);
        // Buy-and-hold 1000/100 = 10 shares, end at 120 → NAV = 1200
        assert!((result.benchmark.total_return - 0.2).abs() < 1e-9);
    }
}
