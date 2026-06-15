use serde::Serialize;
use crate::data::Candle;
use crate::strategy::{Signal, Strategy};
use crate::portfolio::{EquityPoint, Portfolio, TradeRecord};
use crate::metrics::Metrics;

#[derive(Debug, Serialize, Clone)]
pub struct BacktestResult {
    pub equity_curve: Vec<EquityPoint>,
    pub trades: Vec<TradeRecord>,
    pub metrics: Metrics,
}

pub struct BacktestEngine<S: Strategy> {
    strategy: S,
    portfolio: Portfolio,
}

impl<S: Strategy> BacktestEngine<S> {
    pub fn new(strategy: S, portfolio: Portfolio) -> Self {
        Self { strategy, portfolio }
    }

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
