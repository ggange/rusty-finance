pub mod rsi;
pub mod ma;

use std::collections::VecDeque;
use serde::Serialize;
use crate::data::Candle;

pub trait Strategy {
    fn on_bar(&mut self, candle: &Candle) -> Signal;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Signal {
    Buy,
    Sell,
    Hold,
}

pub(crate) trait VecDequeExt {
    fn make_contiguous_copy(&self) -> Vec<f64>;
}

impl VecDequeExt for VecDeque<f64> {
    fn make_contiguous_copy(&self) -> Vec<f64> {
        self.iter().copied().collect()
    }
}
