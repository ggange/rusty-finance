use serde::Serialize;
use crate::portfolio::{EquityPoint, TradeRecord};

/// Buy-and-hold benchmark: total return and CAGR for holding `initial_cash`
/// worth of the asset from `first_price` to `last_price` over `n_bars` trading days.
#[derive(Debug, Serialize, Clone)]
pub struct Benchmark {
    pub total_return: f64,
    pub cagr: f64,
}

impl Benchmark {
    pub fn compute(initial_cash: f64, first_price: f64, last_price: f64, n_bars: usize) -> Self {
        if first_price <= 0.0 || n_bars == 0 {
            return Self { total_return: 0.0, cagr: 0.0 };
        }
        let shares = (initial_cash / first_price) as u32;
        let cash_rem = initial_cash - shares as f64 * first_price;
        let end_nav = shares as f64 * last_price + cash_rem;
        let total_return = end_nav / initial_cash - 1.0;
        let cagr = (end_nav / initial_cash).powf(252.0 / n_bars as f64) - 1.0;
        Self { total_return, cagr }
    }
}

/// Aggregate performance statistics derived from an equity curve and trade log.
#[derive(Debug, Serialize, Clone)]
pub struct Metrics {
    /// `last_nav / first_nav − 1`.
    pub total_return: f64,
    /// Compound annual growth rate, assuming 252 trading days/year.
    pub cagr: f64,
    /// Annualized standard deviation of daily returns.
    pub annualized_volatility: f64,
    /// Most negative peak-to-trough ratio.
    pub max_drawdown: f64,
    /// Annualized Sharpe ratio (rf = 0, 252 days).
    pub sharpe_ratio: f64,
    /// Annualized Sortino ratio (penalizes downside volatility only).
    pub sortino_ratio: f64,
    /// Fraction of completed sell trades that were profitable. `None` if no sells.
    pub win_rate: Option<f64>,
    /// Total number of executed trades (buys + sells).
    pub trade_count: usize,
}

impl Metrics {
    /// Compute all metrics. Returns zeroed struct if the curve has fewer than 2 points.
    pub fn compute(curve: &[EquityPoint], trades: &[TradeRecord]) -> Self {
        let zero = Self {
            total_return: 0.0, cagr: 0.0, annualized_volatility: 0.0,
            max_drawdown: 0.0, sharpe_ratio: 0.0, sortino_ratio: 0.0,
            win_rate: None, trade_count: trades.len(),
        };
        if curve.len() < 2 { return zero; }

        let first_nav = curve[0].nav;
        let last_nav = curve[curve.len() - 1].nav;
        let n = curve.len();
        let total_return = last_nav / first_nav - 1.0;
        let cagr = (last_nav / first_nav).powf(252.0 / n as f64) - 1.0;

        // Max drawdown
        let mut peak = first_nav;
        let mut max_drawdown = 0.0_f64;
        for ep in curve {
            if ep.nav > peak { peak = ep.nav; }
            let dd = (ep.nav - peak) / peak;
            if dd < max_drawdown { max_drawdown = dd; }
        }

        // Daily returns
        let returns: Vec<f64> = curve.windows(2)
            .map(|w| w[1].nav / w[0].nav - 1.0)
            .collect();
        let nr = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / nr;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / nr;
        let std_dev = variance.sqrt();

        let annualized_volatility = std_dev * 252_f64.sqrt();
        let sharpe_ratio = if std_dev == 0.0 { 0.0 } else { mean / std_dev * 252_f64.sqrt() };

        // Sortino: downside deviation (returns below 0)
        let downside_var = returns.iter()
            .map(|&r| if r < 0.0 { r * r } else { 0.0 })
            .sum::<f64>() / nr;
        let downside_dev = downside_var.sqrt();
        let sortino_ratio = if downside_dev == 0.0 { 0.0 } else { mean / downside_dev * 252_f64.sqrt() };

        // Win rate: profitable sell trades / total sell trades
        let sells: Vec<_> = trades.iter().filter_map(|t| t.pnl).collect();
        let win_rate = if sells.is_empty() {
            None
        } else {
            let wins = sells.iter().filter(|&&p| p > 0.0).count();
            Some(wins as f64 / sells.len() as f64)
        };

        Self {
            total_return, cagr, annualized_volatility, max_drawdown,
            sharpe_ratio, sortino_ratio, win_rate, trade_count: trades.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn ep(day: u32, nav: f64) -> EquityPoint {
        EquityPoint { date: NaiveDate::from_ymd_opt(2024, 1, day).unwrap(), nav }
    }

    #[test]
    fn total_return_25_pct() {
        let curve = vec![ep(1, 10000.0), ep(2, 10000.0), ep(3, 12500.0)];
        let m = Metrics::compute(&curve, &[]);
        assert!((m.total_return - 0.25).abs() < 1e-9);
    }

    #[test]
    fn max_drawdown_one_third() {
        let curve = vec![ep(1, 10000.0), ep(2, 12000.0), ep(3, 8000.0), ep(4, 10000.0)];
        let m = Metrics::compute(&curve, &[]);
        assert!((m.max_drawdown - (-1.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn zero_variance_sharpe_and_sortino_are_zero() {
        let curve = vec![ep(1, 10000.0), ep(2, 10000.0), ep(3, 10000.0)];
        let m = Metrics::compute(&curve, &[]);
        assert_eq!(m.sharpe_ratio, 0.0);
        assert_eq!(m.sortino_ratio, 0.0);
    }

    #[test]
    fn single_point_curve_returns_zeroes() {
        let m = Metrics::compute(&[ep(1, 10000.0)], &[]);
        assert_eq!(m.total_return, 0.0);
        assert_eq!(m.max_drawdown, 0.0);
    }

    #[test]
    fn win_rate_none_when_no_sells() {
        let curve = vec![ep(1, 10000.0), ep(2, 11000.0)];
        let m = Metrics::compute(&curve, &[]);
        assert!(m.win_rate.is_none());
    }

    #[test]
    fn win_rate_computed_from_pnl() {
        use crate::strategy::Signal;
        use crate::portfolio::TradeRecord;
        let curve = vec![ep(1, 10000.0), ep(2, 11000.0), ep(3, 10000.0)];
        let trades = vec![
            TradeRecord { date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(), action: Signal::Sell, shares: 10, price: 110.0, commission: 0.0, cash_after: 1100.0, pnl: Some(100.0) },
            TradeRecord { date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(), action: Signal::Sell, shares: 10, price: 90.0, commission: 0.0, cash_after: 900.0, pnl: Some(-100.0) },
        ];
        let m = Metrics::compute(&curve, &trades);
        assert!((m.win_rate.unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn benchmark_computes_buy_and_hold_return() {
        // 10000 cash, buy at 100 → 100 shares. End at 150 → NAV = 15000.
        let b = Benchmark::compute(10000.0, 100.0, 150.0, 252);
        assert!((b.total_return - 0.5).abs() < 1e-9);
    }

    #[test]
    fn benchmark_zero_price_returns_zero() {
        let b = Benchmark::compute(10000.0, 0.0, 150.0, 252);
        assert_eq!(b.total_return, 0.0);
    }

    #[test]
    fn cagr_equals_total_return_over_one_year() {
        // Over exactly 252 bars, CAGR == total_return
        let b = Benchmark::compute(10000.0, 100.0, 150.0, 252);
        assert!((b.cagr - b.total_return).abs() < 1e-6);
    }
}
