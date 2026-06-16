use std::collections::VecDeque;
use crate::strategy::{Signal, Strategy, VecDequeExt};
use crate::data::Candle;

/// Relative Strength Index strategy using Wilder smoothing.
///
/// Emits [`Signal::Buy`] when RSI < 30 (oversold) and [`Signal::Sell`] when RSI > 70 (overbought).
/// No signal is emitted during the initial warmup of `period - 1` bars.
pub struct RSI {
    period: usize,
    gains: VecDeque<f64>,
    losses: VecDeque<f64>,
    prev_close: Option<f64>,
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
            gains: VecDeque::with_capacity(period),
            losses: VecDeque::with_capacity(period),
            prev_close: None,
        }
    }

    fn calculate_rsi(&self) -> f64 {
        let avg_gain = wilder_ma(self.gains.make_contiguous_copy(), self.period);
        let avg_loss = wilder_ma(self.losses.make_contiguous_copy(), self.period);

        if avg_loss == 0.0 {
            return 100.0;
        }
        if avg_gain == 0.0 {
            return 0.0;
        }

        let rs = avg_gain / avg_loss;
        100.0 - (100.0 / (1.0 + rs))
    }
}

impl Strategy for RSI {
    fn on_bar(&mut self, candle: &Candle) -> Signal {
        if let Some(prev) = self.prev_close {
            let change = candle.close - prev;
            if change > 0.0 {
                self.gains.push_back(change);
                self.losses.push_back(0.0);
            } else {
                self.gains.push_back(0.0);
                self.losses.push_back(-change);
            }
        } else {
            self.gains.push_back(0.0);
            self.losses.push_back(0.0);
        }

        self.prev_close = Some(candle.close);

        if self.gains.len() > self.period {
            self.gains.pop_front();
            self.losses.pop_front();
        }

        if self.gains.len() == self.period {
            let rsi = self.calculate_rsi();
            if rsi > 70.0 {
                return Signal::Sell;
            } else if rsi < 30.0 {
                return Signal::Buy;
            }
        }

        Signal::Hold
    }
}

fn wilder_ma(values: Vec<f64>, period: usize) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().skip(1).fold(values[0], |wma, &v| {
        (wma * (period as f64 - 1.0) + v) / period as f64
    })
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

    #[test]
    fn hold_during_warmup_period() {
        // period=7: bars 1-6 have gains.len() < 7, so no RSI is computed → Hold
        let s = signals(7, &[100.0; 6]);
        assert!(s.iter().all(|sig| *sig == Signal::Hold));
    }

    #[test]
    fn sell_signal_when_rsi_above_70() {
        // period=4: 1 flat bar seeds prev_close, then 3 large gains → gains=[0,10,10,10], losses=[0,0,0,0]
        // avg_loss=0 → RSI=100 → Sell
        let s = signals(4, &[100.0, 110.0, 120.0, 130.0]);
        assert!(s.iter().any(|sig| *sig == Signal::Sell), "expected at least one Sell");
    }

    #[test]
    fn buy_signal_when_rsi_below_30() {
        // period=4: 1 flat bar, then 3 large drops → gains=[0,0,0,0], losses=[0,10,10,10]
        // avg_gain=0 → RSI=0 → Buy
        let s = signals(4, &[100.0, 90.0, 80.0, 70.0]);
        assert!(s.iter().any(|sig| *sig == Signal::Buy), "expected at least one Buy");
    }

    #[test]
    fn wilder_smoothing_produces_hold_when_rsi_near_40() {
        // period=3, sequence [100, 100, 103, 100]:
        //   bar 3: gains=[0,0,3], losses=[0,0,0] → avg_loss=0 → RSI=100 → Sell
        //   bar 4: gains=[0,3,0], losses=[0,0,3]
        //     avg_gain = Wilder([0,3,0]) = fold(0,[3,0]) = (0*2+3)/3=1.0 → (1.0*2+0)/3=0.667
        //     avg_loss = Wilder([0,0,3]) = fold(0,[0,3]) = 0 → (0*2+3)/3=1.0
        //     RSI = 100 - 100/(1+0.667) ≈ 40.0 → Hold
        let s = signals(3, &[100.0, 100.0, 103.0, 100.0]);
        assert_eq!(s[2], Signal::Sell, "bar 3: all gains → RSI=100");
        assert_eq!(s[3], Signal::Hold, "bar 4: RSI≈40 is between 30 and 70");
    }

    #[test]
    #[should_panic(expected = "RSI period must be greater than 1")]
    fn rsi_period_1_panics() {
        RSI::new(1);
    }
}
