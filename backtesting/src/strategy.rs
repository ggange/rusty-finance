pub mod rsi;
pub mod ma;
pub mod macd;
pub mod bollinger_bands;

use serde::Serialize;
use crate::data::Candle;

/// Trait that a trading strategy must implement.
///
/// `on_bar` is called once per candle in chronological order. Strategies are stateful and
/// accumulate internal history (e.g. moving average windows) across calls.
pub trait Strategy {
    /// Evaluate the current bar and return a trading signal.
    fn on_bar(&mut self, candle: &Candle) -> Signal;
}

/// Let a boxed trait object satisfy [`Strategy`] itself, so heterogeneous
/// strategies (e.g. an MA crossover and an RSI) can be stored together as
/// `Box<dyn Strategy>` and still drive a generic [`crate::engine::BacktestEngine`].
impl Strategy for Box<dyn Strategy> {
    fn on_bar(&mut self, candle: &Candle) -> Signal {
        (**self).on_bar(candle)
    }
}

/// The action a strategy recommends for the current bar.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Signal {
    /// Enter a long position (buy as many shares as cash allows).
    Buy,
    /// Exit the long position (sell all shares held).
    Sell,
    /// Take no action this bar.
    Hold,
}

