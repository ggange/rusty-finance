use std::collections::VecDeque;
use crate::strategy::{Signal, Strategy, VecDequeExt};
use crate::data::Candle;

pub struct RSI {
    period: usize,
    gains: VecDeque<f64>,
    losses: VecDeque<f64>,
    prev_close: Option<f64>,
}

impl RSI {
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
