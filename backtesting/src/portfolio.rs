use chrono::NaiveDate;
use serde::Serialize;
use crate::strategy::Signal;

#[derive(Debug, Serialize, Clone)]
pub struct EquityPoint {
    pub date: NaiveDate,
    pub nav: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct TradeRecord {
    pub date: NaiveDate,
    pub action: Signal,
    pub shares: u32,
    pub price: f64,
    pub cash_after: f64,
}

#[derive(Debug, Clone)]
pub struct Portfolio {
    pub symbol: String,
    cash: f64,
    shares: u32,
    equity_curve: Vec<EquityPoint>,
    trades: Vec<TradeRecord>,
}

impl Portfolio {
    pub fn new(initial_cash: f64, symbol: String) -> Self {
        Self {
            symbol,
            cash: initial_cash,
            shares: 0,
            equity_curve: Vec::new(),
            trades: Vec::new(),
        }
    }

    /// Buy as many whole shares as cash allows.
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

    /// Liquidate all shares.
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

    pub fn record_nav(&mut self, price: f64, date: NaiveDate) {
        self.equity_curve.push(EquityPoint { date, nav: self.nav(price) });
    }

    pub fn nav(&self, price: f64) -> f64 {
        self.cash + self.shares as f64 * price
    }

    pub fn equity_curve(&self) -> &[EquityPoint] {
        &self.equity_curve
    }

    pub fn trades(&self) -> &[TradeRecord] {
        &self.trades
    }

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
