use chrono::NaiveDate;
use serde::Serialize;
use crate::strategy::Signal;

/// A single point on the equity curve: the portfolio's net asset value on a given date.
#[derive(Debug, Serialize, Clone)]
pub struct EquityPoint {
    pub date: NaiveDate,
    /// Net asset value: cash + shares × price at bar close.
    pub nav: f64,
}

/// A record of a single executed trade.
#[derive(Debug, Serialize, Clone)]
pub struct TradeRecord {
    pub date: NaiveDate,
    /// Whether this was a buy or sell.
    pub action: Signal,
    /// Number of shares transacted.
    pub shares: u32,
    /// Execution price (bar's close).
    pub price: f64,
    /// Cash balance immediately after the trade.
    pub cash_after: f64,
}

/// Tracks cash and share positions; records the NAV history and trade log.
#[derive(Debug, Clone)]
pub struct Portfolio {
    pub symbol: String,
    cash: f64,
    shares: u32,
    equity_curve: Vec<EquityPoint>,
    trades: Vec<TradeRecord>,
}

impl Portfolio {
    /// Create a new portfolio with the given starting cash and ticker symbol.
    pub fn new(initial_cash: f64, symbol: String) -> Self {
        Self {
            symbol,
            cash: initial_cash,
            shares: 0,
            equity_curve: Vec::new(),
            trades: Vec::new(),
        }
    }

    /// Purchase as many whole shares as the current cash balance permits at `price`.
    ///
    /// No-op if `price <= 0` or cash is insufficient for even one share.
    pub fn buy_all(&mut self, price: f64, date: NaiveDate) {
        if price <= 0.0 {
            return;
        }
        let shares = (self.cash / price) as u32;
        if shares == 0 {
            return;
        }
        self.cash -= shares as f64 * price;
        self.shares += shares;
        self.trades.push(TradeRecord {
            date,
            action: Signal::Buy,
            shares,
            price,
            cash_after: self.cash,
        });
    }

    /// Liquidate the entire share position at `price`. No-op if no shares are held.
    pub fn sell_all(&mut self, price: f64, date: NaiveDate) {
        if self.shares == 0 {
            return;
        }
        let shares = self.shares;
        self.cash += shares as f64 * price;
        self.shares = 0;
        self.trades.push(TradeRecord {
            date,
            action: Signal::Sell,
            shares,
            price,
            cash_after: self.cash,
        });
    }

    /// Append the current NAV to the equity curve using `price` as the mark-to-market value.
    pub fn record_nav(&mut self, price: f64, date: NaiveDate) {
        self.equity_curve.push(EquityPoint { date, nav: self.nav(price) });
    }

    /// Compute the current net asset value: cash plus share count times `price`.
    pub fn nav(&self, price: f64) -> f64 {
        self.cash + self.shares as f64 * price
    }

    /// Return the full equity curve recorded so far.
    pub fn equity_curve(&self) -> &[EquityPoint] {
        &self.equity_curve
    }

    /// Return the trade log recorded so far.
    pub fn trades(&self) -> &[TradeRecord] {
        &self.trades
    }

    /// Return the current cash balance.
    pub fn cash(&self) -> f64 {
        self.cash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, d).unwrap()
    }

    #[test]
    fn buy_all_acquires_max_shares() {
        let mut p = Portfolio::new(1000.0, "TEST".to_string());
        p.buy_all(100.0, date(1));
        assert_eq!(p.shares, 10);
        assert!((p.cash() - 0.0).abs() < 1e-9);
        assert_eq!(p.trades().len(), 1);
    }

    #[test]
    fn sell_all_liquidates_position() {
        let mut p = Portfolio::new(1000.0, "TEST".to_string());
        p.buy_all(100.0, date(1));   // 10 shares @ 100
        p.sell_all(150.0, date(2));  // sell 10 @ 150 = 1500
        assert_eq!(p.shares, 0);
        assert!((p.cash() - 1500.0).abs() < 1e-9);
        assert_eq!(p.trades().len(), 2);
    }

    #[test]
    fn equity_curve_tracks_nav_per_bar() {
        let mut p = Portfolio::new(1000.0, "TEST".to_string());
        p.buy_all(100.0, date(1));    // 10 shares, 0 cash
        p.record_nav(100.0, date(1)); // nav = 0 + 10*100 = 1000
        p.record_nav(110.0, date(2)); // nav = 0 + 10*110 = 1100
        let curve = p.equity_curve();
        assert_eq!(curve.len(), 2);
        assert!((curve[0].nav - 1000.0).abs() < 1e-9);
        assert!((curve[1].nav - 1100.0).abs() < 1e-9);
    }

    #[test]
    fn buy_all_no_op_when_price_zero() {
        let mut p = Portfolio::new(1000.0, "TEST".to_string());
        p.buy_all(0.0, date(1));
        assert_eq!(p.shares, 0);
        assert_eq!(p.trades().len(), 0);
    }

    #[test]
    fn sell_all_no_op_when_no_shares() {
        let mut p = Portfolio::new(1000.0, "TEST".to_string());
        p.sell_all(100.0, date(1));
        assert_eq!(p.trades().len(), 0);
        assert!((p.cash() - 1000.0).abs() < 1e-9);
    }
}
