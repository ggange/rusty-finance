use chrono::NaiveDate;
use serde::Serialize;
use crate::strategy::Signal;

// ─── Execution policy ────────────────────────────────────────────────────────

/// How many shares to transact on each signal.
#[derive(Debug, Serialize, Clone)]
pub enum SizingRule {
    /// Spend all available cash on buys; liquidate everything on sells.
    AllIn,
    /// Trade a fixed number of shares (capped by cash / holdings).
    FixedShares(u32),
    /// Trade a fixed notional dollar amount of shares.
    FixedDollar(f64),
    /// Trade a fixed fraction (0–1) of current portfolio NAV.
    FixedFraction(f64),
}

/// Per-trade costs applied on both sides of every execution.
#[derive(Debug, Serialize, Clone)]
pub struct ExecutionCosts {
    /// Flat fee deducted from cash per trade (buy or sell).
    pub commission_per_trade: f64,
    /// One-way price penalty: buys pay `price × (1 + slippage)`,
    /// sells receive `price × (1 − slippage)`.
    pub slippage_pct: f64,
}

impl Default for ExecutionCosts {
    fn default() -> Self {
        Self { commission_per_trade: 0.0, slippage_pct: 0.0 }
    }
}

// ─── Output types ─────────────────────────────────────────────────────────────

/// A single point on the equity curve: portfolio NAV on a given date.
#[derive(Debug, Serialize, Clone)]
pub struct EquityPoint {
    pub date: NaiveDate,
    /// Net asset value: cash + shares × close price.
    pub nav: f64,
}

/// A record of one executed trade.
#[derive(Debug, Serialize, Clone)]
pub struct TradeRecord {
    pub date: NaiveDate,
    pub action: Signal,
    /// Shares transacted.
    pub shares: u32,
    /// Execution price after slippage.
    pub price: f64,
    /// Commission paid for this trade.
    pub commission: f64,
    /// Cash balance immediately after the trade.
    pub cash_after: f64,
    /// Realized P&L for sell trades: `(exec_price − avg_cost) × shares − commission`.
    /// `None` for buy trades.
    pub pnl: Option<f64>,
}

// ─── Portfolio ────────────────────────────────────────────────────────────────

/// Tracks cash, share position, equity curve, and trade log for one symbol.
#[derive(Debug, Clone)]
pub struct Portfolio {
    pub symbol: String,
    pub initial_cash: f64,
    cash: f64,
    shares: u32,
    /// Volume-weighted average cost per share (for realized P&L on sells).
    cost_basis: f64,
    sizing: SizingRule,
    costs: ExecutionCosts,
    equity_curve: Vec<EquityPoint>,
    trades: Vec<TradeRecord>,
}

impl Portfolio {
    pub fn new(initial_cash: f64, symbol: String) -> Self {
        Self {
            symbol,
            initial_cash,
            cash: initial_cash,
            shares: 0,
            cost_basis: 0.0,
            sizing: SizingRule::AllIn,
            costs: ExecutionCosts::default(),
            equity_curve: Vec::new(),
            trades: Vec::new(),
        }
    }

    /// Set the position-sizing rule (builder pattern).
    pub fn with_sizing(mut self, sizing: SizingRule) -> Self {
        self.sizing = sizing;
        self
    }

    /// Set execution costs (builder pattern).
    pub fn with_costs(mut self, costs: ExecutionCosts) -> Self {
        self.costs = costs;
        self
    }

    /// Execute a buy using the configured sizing rule and costs.
    ///
    /// No-op if price ≤ 0, cash is insufficient for even one share after commission,
    /// or the sizing rule resolves to 0 shares.
    pub fn execute_buy(&mut self, price: f64, date: NaiveDate) {
        if price <= 0.0 { return; }
        let exec_price = price * (1.0 + self.costs.slippage_pct);
        let available_cash = self.cash - self.costs.commission_per_trade;
        if available_cash <= 0.0 { return; }

        let max_affordable = (available_cash / exec_price) as u32;
        if max_affordable == 0 { return; }

        let shares = match &self.sizing {
            SizingRule::AllIn => max_affordable,
            SizingRule::FixedShares(n) => (*n).min(max_affordable),
            SizingRule::FixedDollar(d) => ((*d / exec_price) as u32).min(max_affordable),
            SizingRule::FixedFraction(f) => {
                let target = (self.nav(price) * f / exec_price) as u32;
                target.min(max_affordable)
            }
        };
        if shares == 0 { return; }

        let total_cost = shares as f64 * exec_price + self.costs.commission_per_trade;
        self.cash -= total_cost;

        // Update volume-weighted average cost basis
        let prev_value = self.shares as f64 * self.cost_basis;
        let new_value = shares as f64 * exec_price;
        self.shares += shares;
        self.cost_basis = (prev_value + new_value) / self.shares as f64;

        self.trades.push(TradeRecord {
            date,
            action: Signal::Buy,
            shares,
            price: exec_price,
            commission: self.costs.commission_per_trade,
            cash_after: self.cash,
            pnl: None,
        });
    }

    /// Execute a sell using the configured sizing rule and costs.
    ///
    /// No-op if no shares are held or the sizing rule resolves to 0.
    pub fn execute_sell(&mut self, price: f64, date: NaiveDate) {
        if self.shares == 0 { return; }
        let exec_price = price * (1.0 - self.costs.slippage_pct);

        let shares = match &self.sizing {
            SizingRule::AllIn => self.shares,
            SizingRule::FixedShares(n) => (*n).min(self.shares),
            SizingRule::FixedDollar(d) => (((*d) / exec_price) as u32).min(self.shares),
            SizingRule::FixedFraction(f) => {
                let target = (self.nav(price) * f / exec_price) as u32;
                target.min(self.shares)
            }
        };
        if shares == 0 { return; }

        let proceeds = shares as f64 * exec_price - self.costs.commission_per_trade;
        let pnl = (exec_price - self.cost_basis) * shares as f64 - self.costs.commission_per_trade;
        self.cash += proceeds;
        self.shares -= shares;
        if self.shares == 0 { self.cost_basis = 0.0; }

        self.trades.push(TradeRecord {
            date,
            action: Signal::Sell,
            shares,
            price: exec_price,
            commission: self.costs.commission_per_trade,
            cash_after: self.cash,
            pnl: Some(pnl),
        });
    }

    /// Append the current NAV to the equity curve.
    pub fn record_nav(&mut self, price: f64, date: NaiveDate) {
        self.equity_curve.push(EquityPoint { date, nav: self.nav(price) });
    }

    /// Current net asset value: cash + shares × price.
    pub fn nav(&self, price: f64) -> f64 {
        self.cash + self.shares as f64 * price
    }

    pub fn equity_curve(&self) -> &[EquityPoint] { &self.equity_curve }
    pub fn trades(&self) -> &[TradeRecord] { &self.trades }
    pub fn cash(&self) -> f64 { self.cash }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, d).unwrap()
    }

    fn portfolio() -> Portfolio {
        Portfolio::new(1000.0, "TEST".to_string())
    }

    #[test]
    fn execute_buy_acquires_max_shares() {
        let mut p = portfolio();
        p.execute_buy(100.0, date(1));
        assert_eq!(p.shares, 10);
        assert!((p.cash() - 0.0).abs() < 1e-9);
        assert_eq!(p.trades().len(), 1);
        assert!(p.trades()[0].pnl.is_none());
    }

    #[test]
    fn execute_sell_liquidates_position_and_records_pnl() {
        let mut p = portfolio();
        p.execute_buy(100.0, date(1));   // 10 shares @ 100, cost_basis = 100
        p.execute_sell(150.0, date(2));  // sell 10 @ 150
        assert_eq!(p.shares, 0);
        assert!((p.cash() - 1500.0).abs() < 1e-9);
        let pnl = p.trades()[1].pnl.unwrap();
        assert!((pnl - 500.0).abs() < 1e-9, "pnl = {pnl}"); // (150-100)*10
    }

    #[test]
    fn commission_is_deducted_on_both_sides() {
        let costs = ExecutionCosts { commission_per_trade: 5.0, slippage_pct: 0.0 };
        let mut p = Portfolio::new(1000.0, "T".to_string()).with_costs(costs);
        p.execute_buy(100.0, date(1));  // pays 5 commission + shares
        // max affordable: (1000 - 5) / 100 = 9 shares, cost = 9*100 + 5 = 905
        assert_eq!(p.shares, 9);
        assert!((p.cash() - 95.0).abs() < 1e-9);
        p.execute_sell(100.0, date(2)); // proceeds = 9*100 - 5 = 895
        assert!((p.cash() - 990.0).abs() < 1e-9);
    }

    #[test]
    fn slippage_worsens_execution_price() {
        let costs = ExecutionCosts { commission_per_trade: 0.0, slippage_pct: 0.01 };
        let mut p = Portfolio::new(1000.0, "T".to_string()).with_costs(costs);
        p.execute_buy(100.0, date(1)); // exec @ 101
        assert_eq!(p.trades()[0].price, 101.0);
        p.execute_sell(100.0, date(2)); // exec @ 99
        assert_eq!(p.trades()[1].price, 99.0);
    }

    #[test]
    fn fixed_shares_sizing_buys_exactly_n() {
        let mut p = Portfolio::new(10_000.0, "T".to_string())
            .with_sizing(SizingRule::FixedShares(3));
        p.execute_buy(100.0, date(1));
        assert_eq!(p.shares, 3);
    }

    #[test]
    fn fixed_dollar_sizing_buys_correct_shares() {
        let mut p = Portfolio::new(10_000.0, "T".to_string())
            .with_sizing(SizingRule::FixedDollar(500.0));
        p.execute_buy(100.0, date(1)); // 500/100 = 5 shares
        assert_eq!(p.shares, 5);
    }

    #[test]
    fn equity_curve_tracks_nav_per_bar() {
        let mut p = portfolio();
        p.execute_buy(100.0, date(1));    // 10 shares, 0 cash
        p.record_nav(100.0, date(1));     // nav = 1000
        p.record_nav(110.0, date(2));     // nav = 1100
        let curve = p.equity_curve();
        assert!((curve[0].nav - 1000.0).abs() < 1e-9);
        assert!((curve[1].nav - 1100.0).abs() < 1e-9);
    }

    #[test]
    fn execute_buy_no_op_when_price_zero() {
        let mut p = portfolio();
        p.execute_buy(0.0, date(1));
        assert_eq!(p.shares, 0);
        assert_eq!(p.trades().len(), 0);
    }

    #[test]
    fn execute_sell_no_op_when_no_shares() {
        let mut p = portfolio();
        p.execute_sell(100.0, date(1));
        assert_eq!(p.trades().len(), 0);
        assert!((p.cash() - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn cost_basis_updates_on_multiple_buys() {
        let mut p = Portfolio::new(2000.0, "T".to_string())
            .with_sizing(SizingRule::FixedShares(5));
        p.execute_buy(100.0, date(1)); // 5 shares @ 100, basis = 100
        p.execute_buy(200.0, date(2)); // 5 more shares @ 200, basis = (500+1000)/10 = 150
        // FixedShares(5) sells 5 shares @ 200: pnl = (200 - 150) * 5 = 250
        p.execute_sell(200.0, date(3));
        let pnl = p.trades().last().unwrap().pnl.unwrap();
        assert!((pnl - 250.0).abs() < 1e-9, "pnl = {pnl}");
    }
}
