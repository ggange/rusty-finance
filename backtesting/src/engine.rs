use serde::Serialize;
use crate::data::Candle;
use crate::strategy::{Signal, Strategy};
use crate::portfolio::{EquityPoint, Portfolio, TradeRecord};
use crate::metrics::Metrics;

/// The complete output of a backtest run.
#[derive(Debug, Serialize, Clone)]
pub struct BacktestResult {
    /// NAV recorded after every bar, in chronological order.
    pub equity_curve: Vec<EquityPoint>,
    /// Every executed trade in the order it occurred.
    pub trades: Vec<TradeRecord>,
    /// Aggregate performance statistics derived from the equity curve.
    pub metrics: Metrics,
}

/// Drives a backtest by feeding candles to a [`Strategy`] and executing signals against a [`Portfolio`].
pub struct BacktestEngine<S: Strategy> {
    strategy: S,
    portfolio: Portfolio,
}

impl<S: Strategy> BacktestEngine<S> {
    /// Create a new engine with the given strategy and portfolio.
    pub fn new(strategy: S, portfolio: Portfolio) -> Self {
        Self { strategy, portfolio }
    }

    /// Process a slice of candles in chronological order.
    ///
    /// For each bar the strategy is consulted; the resulting signal is executed against the
    /// portfolio, then the current NAV is appended to the equity curve.
    pub fn run(&mut self, candles: &[Candle]) {
        for candle in candles {
            let signal = self.strategy.on_bar(candle);
            match signal {
                Signal::Buy  => self.portfolio.buy_all(candle.close, candle.date),
                Signal::Sell => self.portfolio.sell_all(candle.close, candle.date),
                Signal::Hold => (),
            }
            self.portfolio.record_nav(candle.close, candle.date);
        }
    }

    /// Consume the engine and return the full backtest result with metrics.
    pub fn result(self) -> BacktestResult {
        let metrics = Metrics::compute(self.portfolio.equity_curve());
        BacktestResult {
            equity_curve: self.portfolio.equity_curve().to_vec(),
            trades: self.portfolio.trades().to_vec(),
            metrics,
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
        // Buy at 100 → 10 shares, 0 cash. Hold at 150 → NAV = 10 * 150 = 1500
        let result = run(
            vec![Signal::Buy, Signal::Hold],
            &[100.0, 150.0],
            1000.0,
        );
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
        // Buy at 100, end at 200 → total_return = 2000/1000 - 1 = 1.0
        let result = run(
            vec![Signal::Buy, Signal::Hold],
            &[100.0, 200.0],
            1000.0,
        );
        assert!((result.metrics.total_return - 1.0).abs() < 1e-9);
    }
}
