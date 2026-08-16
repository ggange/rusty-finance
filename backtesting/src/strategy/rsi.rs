use crate::strategy::{Signal, Strategy};
use crate::data::Candle;

/// Relative Strength Index strategy using Wilder's smoothing.
///
/// Emits [`Signal::Buy`] when RSI < 30 (oversold) and [`Signal::Sell`] when
/// RSI > 70 (overbought).
///
/// # Warm-up
///
/// RSI needs `period` price *changes*, which takes `period + 1` bars — the first
/// bar establishes a reference close and produces no change. [`Signal::Hold`] is
/// returned for the first `period` bars and [`RSI::value`] is `None` over them.
///
/// # Smoothing
///
/// Wilder's average is a running exponential average seeded once from the simple
/// mean of the first `period` changes, then advanced as
/// `avg = (avg × (period − 1) + change) / period` on every subsequent bar. It
/// therefore depends on the entire history the strategy has seen, not only the
/// trailing `period` bars — two series with identical recent changes but
/// different histories will report different RSI, which is the defining property
/// of Wilder's formulation and the reason it is not a windowed average.
pub struct RSI {
    period: usize,
    prev_close: Option<f64>,
    /// The first `period` changes, accumulated to seed the running averages.
    seed_gains: Vec<f64>,
    seed_losses: Vec<f64>,
    /// Running Wilder averages. `None` until the seed is full.
    avg_gain: Option<f64>,
    avg_loss: Option<f64>,
}

impl RSI {
    /// Create a new RSI strategy with the given lookback period.
    ///
    /// # Panics
    ///
    /// Panics if `period <= 1`.
    pub fn new(period: usize) -> Self {
        assert!(period > 1, "RSI period must be greater than 1");
        Self {
            period,
            prev_close: None,
            seed_gains: Vec::with_capacity(period),
            seed_losses: Vec::with_capacity(period),
            avg_gain: None,
            avg_loss: None,
        }
    }

    /// The current RSI value, or `None` while still warming up.
    pub fn value(&self) -> Option<f64> {
        match (self.avg_gain, self.avg_loss) {
            (Some(avg_gain), Some(avg_loss)) => Some(rsi_from(avg_gain, avg_loss)),
            _ => None,
        }
    }
}

impl Strategy for RSI {
    fn on_bar(&mut self, candle: &Candle) -> Signal {
        let Some(prev) = self.prev_close.replace(candle.close) else {
            // First bar: no previous close, so no change to record.
            return Signal::Hold;
        };

        let change = candle.close - prev;
        let (gain, loss) = if change > 0.0 { (change, 0.0) } else { (0.0, -change) };

        match (self.avg_gain, self.avg_loss) {
            // Seeded: advance the running Wilder averages.
            (Some(avg_gain), Some(avg_loss)) => {
                let p = self.period as f64;
                self.avg_gain = Some((avg_gain * (p - 1.0) + gain) / p);
                self.avg_loss = Some((avg_loss * (p - 1.0) + loss) / p);
            }
            // Still seeding: collect changes, then seed from their simple mean.
            _ => {
                self.seed_gains.push(gain);
                self.seed_losses.push(loss);
                if self.seed_gains.len() == self.period {
                    let p = self.period as f64;
                    self.avg_gain = Some(self.seed_gains.iter().sum::<f64>() / p);
                    self.avg_loss = Some(self.seed_losses.iter().sum::<f64>() / p);
                }
            }
        }

        match self.value() {
            Some(rsi) if rsi > 70.0 => Signal::Sell,
            Some(rsi) if rsi < 30.0 => Signal::Buy,
            _ => Signal::Hold,
        }
    }
}

/// RSI from Wilder averages, with the boundary cases named.
fn rsi_from(avg_gain: f64, avg_loss: f64) -> f64 {
    if avg_loss == 0.0 {
        // No downside over the window. A *flat* window has no upside either,
        // and no movement is neutral — not maximally overbought. Returning 100
        // here makes a halted or thinly traded symbol with repeated identical
        // closes emit Sell, which is reachable on real data.
        return if avg_gain == 0.0 { 50.0 } else { 100.0 };
    }
    // avg_gain == 0 falls out of the general formula as 0.
    let rs = avg_gain / avg_loss;
    100.0 - (100.0 / (1.0 + rs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn candle(close: f64, day: u32) -> Candle {
        Candle {
            date: NaiveDate::from_ymd_opt(2024, 1, day).unwrap(),
            open: close, high: close + 1.0, low: close - 1.0, close, volume: 0,
        }
    }

    fn signals(period: usize, closes: &[f64]) -> Vec<Signal> {
        let mut rsi = RSI::new(period);
        closes.iter().enumerate()
            .map(|(i, &c)| rsi.on_bar(&candle(c, i as u32 + 1)))
            .collect()
    }

    /// Feed `closes` and return the final RSI value.
    fn final_value(period: usize, closes: &[f64]) -> Option<f64> {
        let mut rsi = RSI::new(period);
        for (i, &c) in closes.iter().enumerate() {
            rsi.on_bar(&candle(c, i as u32 + 1));
        }
        rsi.value()
    }

    #[test]
    fn hold_during_warmup_period() {
        let s = signals(7, &[100.0; 6]);
        assert!(s.iter().all(|sig| *sig == Signal::Hold));
    }

    #[test]
    fn warmup_needs_period_changes_not_period_bars() {
        // `period` changes take `period + 1` bars, because the first bar only
        // establishes a reference close. Seeding a synthetic neutral change for
        // that first bar — as this used to — computes the first RSI from
        // `period - 1` real observations plus one fabricated one.
        let period = 14;
        let closes: Vec<f64> = (0..16).map(|i| 100.0 + i as f64).collect();

        let mut rsi = RSI::new(period);
        for (i, &c) in closes.iter().take(period).enumerate() {
            rsi.on_bar(&candle(c, i as u32 + 1));
            assert!(rsi.value().is_none(), "bar {} should still be warming up", i + 1);
        }
        rsi.on_bar(&candle(closes[period], period as u32 + 1));
        assert!(rsi.value().is_some(), "bar {} completes the seed", period + 1);
    }

    #[test]
    fn sell_signal_when_rsi_above_70() {
        // period=4 needs 4 changes → 5 bars. All gains → avg_loss = 0 → RSI 100.
        let s = signals(4, &[100.0, 110.0, 120.0, 130.0, 140.0]);
        assert!(s.iter().any(|sig| *sig == Signal::Sell), "expected at least one Sell");
    }

    #[test]
    fn buy_signal_when_rsi_below_30() {
        let s = signals(4, &[100.0, 90.0, 80.0, 70.0, 60.0]);
        assert!(s.iter().any(|sig| *sig == Signal::Buy), "expected at least one Buy");
    }

    #[test]
    fn a_flat_series_is_neutral_not_overbought() {
        // Every change is zero, so there is no downside *and* no upside. That is
        // RSI 50 by convention. Checking avg_loss == 0 first and returning 100
        // made a flat window emit Sell — reachable on real data whenever a
        // symbol is halted or thinly traded enough to repeat a close.
        let period = 3;
        let s = signals(period, &[100.0; 10]);
        assert!(
            s.iter().all(|sig| *sig == Signal::Hold),
            "a flat series must not signal, got {s:?}"
        );
        assert_eq!(final_value(period, &[100.0; 10]), Some(50.0));
    }

    #[test]
    fn seeds_from_a_simple_average_then_smooths() {
        // period=3, closes [100, 103, 103, 100] → changes +3, 0, −3.
        //   seed avg_gain = (3 + 0 + 0) / 3 = 1.0
        //   seed avg_loss = (0 + 0 + 3) / 3 = 1.0  → RS = 1 → RSI = 50
        assert_eq!(final_value(3, &[100.0, 103.0, 103.0, 100.0]), Some(50.0));

        // One more bar at +6 advances the running averages:
        //   avg_gain = (1.0 × 2 + 6) / 3 = 8/3
        //   avg_loss = (1.0 × 2 + 0) / 3 = 2/3  → RS = 4 → RSI = 80
        let v = final_value(3, &[100.0, 103.0, 103.0, 100.0, 106.0]).unwrap();
        assert!((v - 80.0).abs() < 1e-9, "expected 80, got {v}");
    }

    #[test]
    fn rsi_depends_on_history_before_the_trailing_window() {
        // Both series end with the same three changes (+1, +1, +1), so a windowed
        // average over the last `period` changes would report an identical RSI.
        // Wilder's does not: the seed carries the earlier move forward forever.
        let period = 3;
        let with_a_crash = final_value(period, &[100.0, 90.0, 91.0, 92.0, 93.0]).unwrap();
        let steady_climb = final_value(period, &[100.0, 101.0, 102.0, 103.0, 104.0]).unwrap();

        assert!(
            with_a_crash < 30.0,
            "the −10 change should still dominate, got {with_a_crash}"
        );
        assert_eq!(steady_climb, 100.0, "all gains → avg_loss = 0 → RSI 100");
        assert!(
            (with_a_crash - steady_climb).abs() > 50.0,
            "a windowed average would have returned the same value for both"
        );
    }

    #[test]
    #[should_panic(expected = "RSI period must be greater than 1")]
    fn rsi_period_1_panics() {
        RSI::new(1);
    }
}
