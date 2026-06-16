use serde::Serialize;
use crate::portfolio::EquityPoint;

/// Aggregate performance statistics derived from an equity curve.
#[derive(Debug, Serialize, Clone)]
pub struct Metrics {
    /// `last_nav / first_nav - 1`. E.g. `0.25` means +25%.
    pub total_return: f64,
    /// Most negative peak-to-trough ratio. E.g. `-0.33` means a 33% drawdown.
    pub max_drawdown: f64,
    /// Annualized Sharpe ratio, assuming zero risk-free rate and 252 trading days per year.
    pub sharpe_ratio: f64,
}

impl Metrics {
    /// Compute metrics from an equity curve. Returns all-zeros if the curve has fewer than 2 points.
    pub fn compute(curve: &[EquityPoint]) -> Self {
        if curve.len() < 2 {
            return Self { total_return: 0.0, max_drawdown: 0.0, sharpe_ratio: 0.0 };
        }

        let first_nav = curve[0].nav;
        let last_nav = curve[curve.len() - 1].nav;
        let total_return = last_nav / first_nav - 1.0;

        let mut peak = curve[0].nav;
        let mut max_drawdown = 0.0_f64;
        for ep in curve {
            if ep.nav > peak {
                peak = ep.nav;
            }
            let dd = (ep.nav - peak) / peak;
            if dd < max_drawdown {
                max_drawdown = dd;
            }
        }

        // Daily returns, annualized Sharpe (rf = 0, factor = 252)
        let returns: Vec<f64> = curve.windows(2)
            .map(|w| w[1].nav / w[0].nav - 1.0)
            .collect();
        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();
        let sharpe_ratio = if std_dev == 0.0 { 0.0 } else { mean / std_dev * 252_f64.sqrt() };

        Self { total_return, max_drawdown, sharpe_ratio }
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
        let m = Metrics::compute(&curve);
        assert!((m.total_return - 0.25).abs() < 1e-9);
    }

    #[test]
    fn max_drawdown_one_third() {
        // Peak = 12000, trough = 8000 → dd = (8000-12000)/12000 = -1/3
        let curve = vec![ep(1, 10000.0), ep(2, 12000.0), ep(3, 8000.0), ep(4, 10000.0)];
        let m = Metrics::compute(&curve);
        assert!((m.max_drawdown - (-1.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn zero_variance_sharpe_is_zero() {
        let curve = vec![ep(1, 10000.0), ep(2, 10000.0), ep(3, 10000.0)];
        let m = Metrics::compute(&curve);
        assert_eq!(m.sharpe_ratio, 0.0);
    }

    #[test]
    fn single_point_curve_returns_zeroes() {
        let curve = vec![ep(1, 10000.0)];
        let m = Metrics::compute(&curve);
        assert_eq!(m.total_return, 0.0);
        assert_eq!(m.max_drawdown, 0.0);
        assert_eq!(m.sharpe_ratio, 0.0);
    }
}
