use std::cmp::Ordering;
use std::collections::VecDeque;
use crate::strategy::{Signal, Strategy};
use crate::data::Candle;

/// Selects the moving average algorithm used by [`MovingAverageCrossover`].
#[derive(Debug, Clone, PartialEq)]
pub enum MAType {
    /// Simple moving average — equal weight to all bars in the window.
    SMA,
    /// Exponential moving average — exponentially decaying weights, seeded on the first full window.
    EMA,
    /// Weighted moving average — linearly increasing weights (most recent bar has highest weight).
    WMA,
}

/// Generates Buy/Sell signals when the short-window MA crosses the long-window MA.
///
/// No signal is emitted until enough bars have accumulated to fill the long window.
/// A crossover is detected by comparing the current short/long relationship to the previous bar's.
pub struct MovingAverageCrossover {
    ma_type: MAType,
    short_window: usize,
    long_window: usize,
    prices: VecDeque<f64>,
    // Running EMA values — seeded on first full window, updated incrementally after
    ema_short: Option<f64>,
    ema_long: Option<f64>,
    // Last observed relationship between short and long MA, for crossover detection
    prev_rel: Option<Ordering>,
}

impl MovingAverageCrossover {
    /// Create a new crossover strategy.
    ///
    /// # Panics
    ///
    /// Panics if `short_window >= long_window` or either window is zero.
    pub fn new(ma_type: MAType, short_window: usize, long_window: usize) -> Self {
        assert!(short_window < long_window, "short_window must be less than long_window");
        assert!(short_window > 0 && long_window > 0, "windows must be non-zero");
        Self {
            ma_type,
            short_window,
            long_window,
            prices: VecDeque::with_capacity(long_window),
            ema_short: None,
            ema_long: None,
            prev_rel: None,
        }
    }
}

impl Strategy for MovingAverageCrossover {
    fn on_bar(&mut self, candle: &Candle) -> Signal {
        let price = candle.close;
        self.prices.push_back(price);
        if self.prices.len() > self.long_window {
            self.prices.pop_front();
        }

        if self.prices.len() < self.long_window {
            return Signal::Hold;
        }

        let long_slice: Vec<f64> = self.prices.iter().copied().collect();
        let short_slice = &long_slice[self.long_window - self.short_window..];

        let (short_avg, long_avg) = match self.ma_type {
            MAType::EMA => {
                let ema_long = match self.ema_long {
                    None => {
                        let seed = sma(&long_slice);
                        self.ema_long = Some(seed);
                        seed
                    }
                    Some(prev) => {
                        let k = 2.0 / (self.long_window as f64 + 1.0);
                        let v = k * price + (1.0 - k) * prev;
                        self.ema_long = Some(v);
                        v
                    }
                };
                let ema_short = match self.ema_short {
                    None => {
                        let seed = sma(short_slice);
                        self.ema_short = Some(seed);
                        seed
                    }
                    Some(prev) => {
                        let k = 2.0 / (self.short_window as f64 + 1.0);
                        let v = k * price + (1.0 - k) * prev;
                        self.ema_short = Some(v);
                        v
                    }
                };
                (ema_short, ema_long)
            }
            MAType::SMA => (sma(short_slice), sma(&long_slice)),
            MAType::WMA => (wma(short_slice), wma(&long_slice)),
        };

        let curr_rel = short_avg.partial_cmp(&long_avg).unwrap_or(Ordering::Equal);
        let prev = self.prev_rel.replace(curr_rel);

        match (prev, curr_rel) {
            (Some(Ordering::Less) | Some(Ordering::Equal), Ordering::Greater) => Signal::Buy,
            (Some(Ordering::Greater) | Some(Ordering::Equal), Ordering::Less) => Signal::Sell,
            _ => Signal::Hold,
        }
    }
}

fn sma(prices: &[f64]) -> f64 {
    prices.iter().sum::<f64>() / prices.len() as f64
}

fn wma(prices: &[f64]) -> f64 {
    let (sum, weight_sum) = prices.iter().enumerate().fold((0.0, 0.0), |(s, ws), (i, &p)| {
        let w = (i + 1) as f64;
        (s + p * w, ws + w)
    });
    sum / weight_sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn candle(close: f64, day: u32) -> Candle {
        Candle {
            date: NaiveDate::from_ymd_opt(2024, 1, day).unwrap(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 0,
        }
    }

    #[test]
    fn sma_crossover_emits_exactly_one_buy_and_one_sell() {
        let mut s = MovingAverageCrossover::new(MAType::SMA, 2, 4);

        // Bars 1-4: prices all 1.0 — warmup, SMA(2)=SMA(4)=1 → Hold
        let mut signals: Vec<Signal> = (1..=4).map(|d| s.on_bar(&candle(1.0, d))).collect();

        // Bar 5: prices=[1,1,1,10] → SMA(2)=[1,10]=5.5 > SMA(4)=3.25 → cross up → Buy
        signals.push(s.on_bar(&candle(10.0, 5)));
        // Bar 6: prices=[1,1,10,10] → SMA(2)=10 > SMA(4)=5.5 → same side → Hold
        signals.push(s.on_bar(&candle(10.0, 6)));
        // Bar 7: prices=[1,10,10,0] → SMA(2)=[10,0]=5 < SMA(4)=5.25 → cross down → Sell
        signals.push(s.on_bar(&candle(0.0, 7)));
        // Bar 8: prices=[10,10,0,0] → SMA(2)=0 < SMA(4)=5 → same side → Hold
        signals.push(s.on_bar(&candle(0.0, 8)));

        let buys = signals.iter().filter(|&s| *s == Signal::Buy).count();
        let sells = signals.iter().filter(|&s| *s == Signal::Sell).count();
        assert_eq!(buys, 1, "expected 1 Buy, got {buys}");
        assert_eq!(sells, 1, "expected 1 Sell, got {sells}");
    }

    #[test]
    fn hold_during_warmup() {
        let mut s = MovingAverageCrossover::new(MAType::SMA, 2, 4);
        for d in 1..=3 {
            assert_eq!(s.on_bar(&candle(100.0, d)), Signal::Hold);
        }
    }
}
