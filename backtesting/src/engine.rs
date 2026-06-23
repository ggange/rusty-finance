use serde::{Deserialize, Serialize};
use crate::data::Candle;
use crate::strategy::{Signal, Strategy};
use crate::portfolio::{EquityPoint, Portfolio, TradeRecord};
use crate::metrics::{Benchmark, Metrics};

/// Controls whether a signal generated on bar N is filled at bar N's close
/// (legacy / lookahead-light) or at bar N+1's open (realistic live behaviour).
///
/// Default is `NextOpen`: a signal fires at the close of bar N and the order
/// fills at the open of bar N+1, matching what a live system would do.
/// A signal generated on the **final bar** under `NextOpen` has no next bar
/// and is therefore **not filled**.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FillTiming {
    /// Fill at the same bar's close that generated the signal. Preserves
    /// legacy results; introduces a one-bar look-ahead.
    Close,
    /// Fill at the next bar's open. Realistic default. A signal on the last
    /// bar is dropped (no next bar exists to fill against).
    NextOpen,
}

impl Default for FillTiming {
    fn default() -> Self { FillTiming::NextOpen }
}

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
    fill_timing: FillTiming,
    first_price: Option<f64>,
    last_price: Option<f64>,
}

impl<S: Strategy> BacktestEngine<S> {
    /// Create a new engine with the default fill timing (`NextOpen`).
    pub fn new(strategy: S, portfolio: Portfolio) -> Self {
        Self { strategy, portfolio, fill_timing: FillTiming::default(), first_price: None, last_price: None }
    }

    /// Override the fill timing. Mirror of `Portfolio::with_costs`.
    pub fn with_fill_timing(mut self, timing: FillTiming) -> Self {
        self.fill_timing = timing;
        self
    }

    /// Feed candles in chronological order. May be called multiple times to
    /// stream data in chunks; the equity curve and trade log accumulate.
    ///
    /// # Fill timing
    /// - `Close`: signal on bar N → fill at bar N's close (legacy).
    /// - `NextOpen`: signal on bar N → fill at bar N+1's open (realistic).
    ///   A signal on the final bar is not filled.
    pub fn run(&mut self, candles: &[Candle]) {
        match self.fill_timing {
            FillTiming::Close => self.run_close(candles),
            FillTiming::NextOpen => self.run_next_open(candles),
        }
    }

    fn run_close(&mut self, candles: &[Candle]) {
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

    fn run_next_open(&mut self, candles: &[Candle]) {
        let mut pending: Option<Signal> = None;
        for (i, candle) in candles.iter().enumerate() {
            if self.first_price.is_none() { self.first_price = Some(candle.close); }
            self.last_price = Some(candle.close);

            // Execute the signal buffered from the previous bar, at this bar's open.
            if let Some(sig) = pending.take() {
                match sig {
                    Signal::Buy  => self.portfolio.execute_buy(candle.open, candle.date),
                    Signal::Sell => self.portfolio.execute_sell(candle.open, candle.date),
                    Signal::Hold => (),
                }
            }

            // Generate a new signal for this bar.
            let signal = self.strategy.on_bar(candle);

            // Buffer it if there is a next bar; drop it if this is the last bar.
            if i + 1 < candles.len() {
                pending = Some(signal);
            }
            // else: last-bar signal is intentionally not filled.

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

    /// Candle where open != close so NextOpen fills differ from Close fills.
    fn candle_oc(open: f64, close: f64, day: u32) -> Candle {
        Candle {
            date: NaiveDate::from_ymd_opt(2024, 1, day).unwrap(),
            open, high: close.max(open), low: close.min(open), close, volume: 0,
        }
    }

    fn candle(close: f64, day: u32) -> Candle {
        candle_oc(close, close, day)
    }

    /// Run with explicit FillTiming; candles have open == close == price.
    fn run_with_timing(signals: Vec<Signal>, closes: &[f64], cash: f64, timing: FillTiming) -> BacktestResult {
        let strategy = MockStrategy::new(signals);
        let portfolio = Portfolio::new(cash, "TEST".to_string());
        let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(timing);
        let candles: Vec<Candle> = closes.iter().enumerate()
            .map(|(i, &c)| candle(c, i as u32 + 1))
            .collect();
        engine.run(&candles);
        engine.result()
    }

    /// Convenience wrapper using FillTiming::Close (legacy behaviour).
    fn run(signals: Vec<Signal>, closes: &[f64], cash: f64) -> BacktestResult {
        run_with_timing(signals, closes, cash, FillTiming::Close)
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
        // Close timing: buy at bar 0's close (100), NAV at bar 0 = 1000 (10 sh * 100).
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

    // ─── FillTiming tests ─────────────────────────────────────────────────────

    #[test]
    fn fill_timing_close_fills_at_same_bar_close() {
        // Signal on bar 0 (close=100) → fills at 100 under Close timing.
        // Bar 1 close=150 → 10 shares * 150 = 1500.
        let result = run_with_timing(
            vec![Signal::Buy, Signal::Hold],
            &[100.0, 150.0],
            1000.0,
            FillTiming::Close,
        );
        assert_eq!(result.trades.len(), 1);
        assert!((result.trades[0].price - 100.0).abs() < 1e-9, "Close: fill price should be 100");
        assert!((result.equity_curve[1].nav - 1500.0).abs() < 1e-9);
    }

    #[test]
    fn fill_timing_next_open_fills_at_next_bar_open() {
        // Bar 0: close=100 → signal Buy; bar 1: open=110, close=150 → fill at 110.
        // Cash=1000, 1000/110 = 9 shares (integer), cash_rem = 1000 - 9*110 = 10.
        // NAV at bar 1 close: 9 * 150 + 10 = 1360.
        let candles = vec![
            candle_oc(90.0, 100.0, 1), // bar 0
            candle_oc(110.0, 150.0, 2), // bar 1: fill at open 110
        ];
        let strategy = MockStrategy::new(vec![Signal::Buy, Signal::Hold]);
        let portfolio = Portfolio::new(1000.0, "TEST".to_string());
        let mut engine = BacktestEngine::new(strategy, portfolio).with_fill_timing(FillTiming::NextOpen);
        engine.run(&candles);
        let result = engine.result();

        assert_eq!(result.trades.len(), 1);
        assert!((result.trades[0].price - 110.0).abs() < 1e-9, "NextOpen: fill price should be 110 (bar 1 open)");
        let shares: f64 = (1000.0_f64 / 110.0).floor();
        let cash_rem = 1000.0 - shares * 110.0;
        let expected_nav = shares * 150.0 + cash_rem;
        assert!((result.equity_curve[1].nav - expected_nav).abs() < 1e-6, "NextOpen NAV mismatch");
    }

    #[test]
    fn fill_timing_next_open_last_bar_signal_not_filled() {
        // Signal on the last bar (bar 2) should not produce a trade.
        let result = run_with_timing(
            vec![Signal::Hold, Signal::Hold, Signal::Buy],
            &[100.0, 100.0, 100.0],
            1000.0,
            FillTiming::NextOpen,
        );
        assert!(result.trades.is_empty(), "last-bar signal must not fill under NextOpen");
    }

    #[test]
    fn fill_timing_next_open_equity_curve_length_unchanged() {
        // NextOpen should still produce one equity point per bar.
        let result = run_with_timing(
            vec![Signal::Buy, Signal::Hold, Signal::Sell],
            &[100.0, 110.0, 120.0],
            1000.0,
            FillTiming::NextOpen,
        );
        assert_eq!(result.equity_curve.len(), 3);
    }

    #[test]
    fn fill_timing_default_is_next_open() {
        assert_eq!(FillTiming::default(), FillTiming::NextOpen);
    }
}
