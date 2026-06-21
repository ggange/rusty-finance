use std::collections::VecDeque;
use crate::strategy::{Signal, Strategy};
use crate::data::Candle;

/// Bollinger Bands mean-reversion strategy.
///
/// Emits Buy when the close price falls below the lower band
/// (SMA − `std_dev_mult` × population σ) and Sell when it rises above
/// the upper band (SMA + `std_dev_mult` × σ).
///
/// An `in_position` flag prevents repeated Buy signals while already long.
/// Warmup: `period` bars before any signal is possible.
pub struct BollingerBands {
    period: usize,
    std_dev_mult: f64,
    /// Close prices, capped at period
    prices: VecDeque<f64>,
    in_position: bool,
}

impl BollingerBands {
    /// # Panics
    ///
    /// Panics if `period < 2` or `std_dev_mult <= 0.0`.
    pub fn new(period: usize, std_dev_mult: f64) -> Self {
        assert!(period >= 2, "period must be >= 2");
        assert!(std_dev_mult > 0.0, "std_dev_mult must be > 0");
        Self {
            period,
            std_dev_mult,
            prices: VecDeque::with_capacity(period),
            in_position: false,
        }
    }
}

impl Strategy for BollingerBands {
    fn on_bar(&mut self, candle: &Candle) -> Signal {
        let price = candle.close;
        self.prices.push_back(price);
        if self.prices.len() > self.period {
            self.prices.pop_front();
        }
        if self.prices.len() < self.period {
            return Signal::Hold;
        }

        let m = mean(&self.prices);
        let sd = std_dev(&self.prices, m);
        let upper = m + self.std_dev_mult * sd;
        let lower = m - self.std_dev_mult * sd;

        if price < lower && !self.in_position {
            self.in_position = true;
            Signal::Buy
        } else if price > upper && self.in_position {
            self.in_position = false;
            Signal::Sell
        } else {
            Signal::Hold
        }
    }
}

fn mean(prices: &VecDeque<f64>) -> f64 {
    prices.iter().sum::<f64>() / prices.len() as f64
}

fn std_dev(prices: &VecDeque<f64>, m: f64) -> f64 {
    let variance = prices.iter().map(|&x| (x - m).powi(2)).sum::<f64>() / prices.len() as f64;
    variance.sqrt()
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
        let mut s = BollingerBands::new(3, 2.0);
        assert_eq!(s.on_bar(&candle(100.0, 1)), Signal::Hold);
        assert_eq!(s.on_bar(&candle(100.0, 2)), Signal::Hold);
    }

    #[test]
    fn buy_when_price_below_lower_band() {
        // Very tight bands (0.01σ) so any slight dip below mean triggers Buy
        let mut s = BollingerBands::new(3, 0.01);
        s.on_bar(&candle(100.0, 1));
        s.on_bar(&candle(100.0, 2));
        // window=[100,100,99.5] → mean≈99.83, σ≈0.236, lower≈99.831 → 99.5 < lower → Buy
        let sig = s.on_bar(&candle(99.5, 3));
        assert_eq!(sig, Signal::Buy);
    }

    #[test]
    fn sell_when_price_above_upper_band() {
        let mut s = BollingerBands::new(3, 0.01);
        s.on_bar(&candle(100.0, 1));
        s.on_bar(&candle(100.0, 2));
        s.on_bar(&candle(99.5, 3)); // Buy
        // window=[100,99.5,100.5] → mean=100, σ≈0.408, upper≈100.004 → 100.5 > upper → Sell
        let sig = s.on_bar(&candle(100.5, 4));
        assert_eq!(sig, Signal::Sell);
    }

    #[test]
    fn no_double_buy() {
        let mut s = BollingerBands::new(3, 0.01);
        s.on_bar(&candle(100.0, 1));
        s.on_bar(&candle(100.0, 2));
        let sig1 = s.on_bar(&candle(99.5, 3)); // Buy
        assert_eq!(sig1, Signal::Buy);
        // Another bar below lower — already in position, must hold
        let sig2 = s.on_bar(&candle(99.5, 4));
        assert_eq!(sig2, Signal::Hold);
    }

    #[test]
    #[should_panic(expected = "period must be >= 2")]
    fn period_one_panics() {
        BollingerBands::new(1, 2.0);
    }

    #[test]
    #[should_panic(expected = "std_dev_mult must be > 0")]
    fn zero_std_dev_mult_panics() {
        BollingerBands::new(20, 0.0);
    }
}
