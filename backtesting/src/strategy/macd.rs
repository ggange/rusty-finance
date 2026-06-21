use std::collections::VecDeque;
use crate::strategy::{Signal, Strategy};
use crate::data::Candle;

/// MACD (Moving Average Convergence Divergence) crossover strategy.
///
/// Emits Buy when the MACD line (fast EMA − slow EMA) crosses above the signal
/// line (EMA of the MACD line), and Sell on the reverse cross. Both EMAs are
/// seeded from SMA on the first full window and updated incrementally.
///
/// Warmup: `slow_period + signal_period − 1` bars before any signal is possible.
pub struct MACD {
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    /// Close prices, capped at slow_period
    prices: VecDeque<f64>,
    fast_ema: Option<f64>,
    slow_ema: Option<f64>,
    /// MACD line values accumulated while seeding the signal EMA
    macd_values: VecDeque<f64>,
    signal_ema: Option<f64>,
    /// Whether MACD was above the signal line on the previous bar
    prev_above: Option<bool>,
}

impl MACD {
    /// # Panics
    ///
    /// Panics if `fast_period >= slow_period`, or if any period is < 2.
    pub fn new(fast_period: usize, slow_period: usize, signal_period: usize) -> Self {
        assert!(fast_period >= 2, "fast_period must be >= 2");
        assert!(slow_period >= 2, "slow_period must be >= 2");
        assert!(signal_period >= 2, "signal_period must be >= 2");
        assert!(fast_period < slow_period, "fast_period must be < slow_period");
        Self {
            fast_period,
            slow_period,
            signal_period,
            prices: VecDeque::with_capacity(slow_period),
            fast_ema: None,
            slow_ema: None,
            macd_values: VecDeque::with_capacity(signal_period),
            signal_ema: None,
            prev_above: None,
        }
    }
}

impl Strategy for MACD {
    fn on_bar(&mut self, candle: &Candle) -> Signal {
        let price = candle.close;
        self.prices.push_back(price);
        if self.prices.len() > self.slow_period {
            self.prices.pop_front();
        }
        if self.prices.len() < self.slow_period {
            return Signal::Hold;
        }

        // Update or seed the fast and slow EMAs
        let prices_vec: Vec<f64> = self.prices.iter().copied().collect();
        let fast_ema = match self.fast_ema {
            None => {
                let seed = sma(&prices_vec[self.slow_period - self.fast_period..]);
                self.fast_ema = Some(seed);
                seed
            }
            Some(prev) => {
                let k = 2.0 / (self.fast_period as f64 + 1.0);
                let v = k * price + (1.0 - k) * prev;
                self.fast_ema = Some(v);
                v
            }
        };
        let slow_ema = match self.slow_ema {
            None => {
                let seed = sma(&prices_vec);
                self.slow_ema = Some(seed);
                seed
            }
            Some(prev) => {
                let k = 2.0 / (self.slow_period as f64 + 1.0);
                let v = k * price + (1.0 - k) * prev;
                self.slow_ema = Some(v);
                v
            }
        };

        let macd = fast_ema - slow_ema;

        // Accumulate MACD values until we can seed the signal EMA
        if self.signal_ema.is_none() {
            self.macd_values.push_back(macd);
            if self.macd_values.len() < self.signal_period {
                return Signal::Hold;
            }
            let macd_vec: Vec<f64> = self.macd_values.iter().copied().collect();
            let seed = sma(&macd_vec);
            self.signal_ema = Some(seed);
            self.prev_above = Some(macd > seed);
            return Signal::Hold; // First bar with full signal — no prior bar to cross from
        }

        // Update signal EMA incrementally
        let k = 2.0 / (self.signal_period as f64 + 1.0);
        let signal_ema = {
            let prev = self.signal_ema.unwrap();
            let v = k * macd + (1.0 - k) * prev;
            self.signal_ema = Some(v);
            v
        };

        let above = macd > signal_ema;
        let sig = match self.prev_above {
            Some(false) if above => Signal::Buy,
            Some(true) if !above => Signal::Sell,
            _ => Signal::Hold,
        };
        self.prev_above = Some(above);
        sig
    }
}

fn sma(prices: &[f64]) -> f64 {
    prices.iter().sum::<f64>() / prices.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn candle(close: f64, day: u32) -> Candle {
        Candle {
            date: NaiveDate::from_ymd_opt(2024, 1, day).unwrap(),
            open: close, high: close, low: close, close,
            volume: 0,
        }
    }

    #[test]
    fn hold_during_warmup() {
        // fast=3, slow=5, signal=3 → warmup = slow + signal - 1 = 7 bars
        let mut s = MACD::new(3, 5, 3);
        for d in 1..=7u32 {
            assert_eq!(s.on_bar(&candle(100.0, d)), Signal::Hold, "expected Hold at bar {d}");
        }
    }

    #[test]
    fn buy_on_upward_crossover_and_sell_on_downward() {
        // Traced scenario with fast=3, slow=5, signal=3:
        // Bars 1-7: price=100 → MACD=0, signal seeded to 0, prev_above=false, Hold each
        // Bar 8:  price=200 → fast_ema=150, slow_ema=133.3, macd=16.7, signal=8.3 → Buy
        // Bar 9:  price=50  → fast_ema=100, slow_ema=105.6, macd=-5.6, signal=1.4 → Sell
        let mut s = MACD::new(3, 5, 3);
        let prices: Vec<f64> = [100.0_f64; 7].iter().chain(&[200.0, 50.0]).copied().collect();
        let signals: Vec<Signal> = prices.iter().enumerate()
            .map(|(i, &p)| s.on_bar(&candle(p, (i + 1) as u32)))
            .collect();
        assert_eq!(signals[7], Signal::Buy,  "expected Buy at bar 8");
        assert_eq!(signals[8], Signal::Sell, "expected Sell at bar 9");
    }

    #[test]
    #[should_panic(expected = "fast_period must be < slow_period")]
    fn fast_gte_slow_panics() {
        MACD::new(5, 5, 3);
    }

    #[test]
    #[should_panic(expected = "signal_period must be >= 2")]
    fn signal_period_one_panics() {
        MACD::new(3, 5, 1);
    }
}
