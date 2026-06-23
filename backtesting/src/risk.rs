use chrono::NaiveDate;
use serde::Serialize;

use crate::portfolio::EquityPoint;

const ROLLING_WINDOW: usize = 21;
const TRADING_DAYS: f64 = 252.0;

#[derive(Debug, Serialize, Clone)]
pub struct RollingPoint {
    pub date: NaiveDate,
    pub value: f64,
}

/// Portfolio risk analytics computed from aligned per-asset return series.
#[derive(Debug, Serialize, Clone)]
pub struct RiskMetrics {
    pub symbols: Vec<String>,
    /// N×N Pearson correlation matrix of daily asset returns.
    pub correlation: Vec<Vec<f64>>,
    /// N×N annualized covariance matrix (×252).
    pub covariance: Vec<Vec<f64>>,
    /// Annualized volatility per asset (×√252).
    pub asset_volatility: Vec<f64>,
    /// β of each asset vs the portfolio return series: cov(r_i, r_p) / var(r_p).
    pub asset_beta: Vec<f64>,
    /// Share of portfolio variance attributable to each asset (sums to ~1).
    pub contribution_to_risk: Vec<f64>,
    /// Annualized portfolio vol over a rolling 21-bar window.
    pub rolling_volatility: Vec<RollingPoint>,
    /// 1-day historical VaR at 95% confidence (negative = loss).
    pub var_95: f64,
    /// Expected shortfall below VaR-95.
    pub cvar_95: f64,
    /// 1-day historical VaR at 99% confidence.
    pub var_99: f64,
    /// Expected shortfall below VaR-99.
    pub cvar_99: f64,
}

impl RiskMetrics {
    fn empty(symbols: &[String]) -> Self {
        let n = symbols.len();
        RiskMetrics {
            symbols: symbols.to_vec(),
            correlation: vec![vec![0.0; n]; n],
            covariance: vec![vec![0.0; n]; n],
            asset_volatility: vec![0.0; n],
            asset_beta: vec![0.0; n],
            contribution_to_risk: vec![0.0; n],
            rolling_volatility: Vec::new(),
            var_95: 0.0,
            cvar_95: 0.0,
            var_99: 0.0,
            cvar_99: 0.0,
        }
    }

    /// Compute risk metrics from pre-aligned data.
    ///
    /// `asset_returns[i]` is asset i's daily return series (all equal length,
    /// aligned to `portfolio_curve` dates). `weights` are the normalized
    /// capital weights (already sum to 1). `portfolio_curve` is the aggregate
    /// equity curve from which portfolio daily returns are derived.
    pub fn compute(
        symbols: &[String],
        asset_returns: &[Vec<f64>],
        weights: &[f64],
        portfolio_curve: &[EquityPoint],
    ) -> Self {
        let n = symbols.len();
        if n == 0 || asset_returns.is_empty() || portfolio_curve.len() < 2 {
            return Self::empty(symbols);
        }

        // Minimum length across all asset return series, also bounded by portfolio len.
        let t = asset_returns
            .iter()
            .map(|r| r.len())
            .min()
            .unwrap_or(0)
            .min(portfolio_curve.len() - 1);
        if t < 2 {
            return Self::empty(symbols);
        }

        // Portfolio daily returns from the aggregate equity curve.
        let port_returns: Vec<f64> = portfolio_curve
            .windows(2)
            .map(|w| if w[0].nav != 0.0 { w[1].nav / w[0].nav - 1.0 } else { 0.0 })
            .collect();

        // --- Covariance & correlation ---

        let means: Vec<f64> = asset_returns
            .iter()
            .map(|r| r[..t].iter().sum::<f64>() / t as f64)
            .collect();

        // Population covariance matrix (daily).
        let mut cov_daily = vec![vec![0.0; n]; n];
        for i in 0..n {
            for j in i..n {
                let c = (0..t)
                    .map(|k| (asset_returns[i][k] - means[i]) * (asset_returns[j][k] - means[j]))
                    .sum::<f64>()
                    / t as f64;
                cov_daily[i][j] = c;
                cov_daily[j][i] = c;
            }
        }

        // Annualized covariance (×252).
        let covariance: Vec<Vec<f64>> = cov_daily
            .iter()
            .map(|row| row.iter().map(|&c| c * TRADING_DAYS).collect())
            .collect();

        // Annualized asset volatility = sqrt(var_daily × 252).
        let asset_volatility: Vec<f64> = (0..n)
            .map(|i| (cov_daily[i][i] * TRADING_DAYS).sqrt())
            .collect();

        // Pearson correlation: corr[i][j] = cov[i][j] / (std_i × std_j).
        let mut correlation = vec![vec![0.0; n]; n];
        for i in 0..n {
            for j in 0..n {
                if cov_daily[i][i] > 0.0 && cov_daily[j][j] > 0.0 {
                    correlation[i][j] =
                        cov_daily[i][j] / (cov_daily[i][i] * cov_daily[j][j]).sqrt();
                } else if i == j {
                    correlation[i][j] = 1.0;
                }
            }
        }

        // --- Asset beta vs portfolio ---

        let port_t = &port_returns[..t.min(port_returns.len())];
        let pt_len = port_t.len() as f64;
        let port_mean = port_t.iter().sum::<f64>() / pt_len;
        let port_var =
            port_t.iter().map(|&r| (r - port_mean).powi(2)).sum::<f64>() / pt_len;

        let asset_beta: Vec<f64> = (0..n)
            .map(|i| {
                if port_var == 0.0 {
                    return 0.0;
                }
                let len = port_t.len().min(t);
                let cov_ip = (0..len)
                    .map(|k| (asset_returns[i][k] - means[i]) * (port_t[k] - port_mean))
                    .sum::<f64>()
                    / len as f64;
                cov_ip / port_var
            })
            .collect();

        // --- Contribution to risk ---
        // σ_p² = wᵀ Σ w.  CCR_i = w_i·(Σw)_i.  Report CCR_i / σ_p² so they sum to 1.
        let sigma_w: Vec<f64> = (0..n)
            .map(|i| (0..n).map(|j| cov_daily[i][j] * weights[j]).sum::<f64>())
            .collect();
        let port_var_w: f64 = (0..n).map(|i| weights[i] * sigma_w[i]).sum();
        let contribution_to_risk: Vec<f64> = if port_var_w <= 0.0 {
            vec![0.0; n]
        } else {
            (0..n)
                .map(|i| weights[i] * sigma_w[i] / port_var_w)
                .collect()
        };

        // --- Rolling portfolio volatility (21-bar window) ---
        let rolling_volatility: Vec<RollingPoint> = if port_returns.len() >= ROLLING_WINDOW {
            (ROLLING_WINDOW..=port_returns.len())
                .map(|end| {
                    let window = &port_returns[end - ROLLING_WINDOW..end];
                    let wmean = window.iter().sum::<f64>() / ROLLING_WINDOW as f64;
                    let wvar = window
                        .iter()
                        .map(|&r| (r - wmean).powi(2))
                        .sum::<f64>()
                        / ROLLING_WINDOW as f64;
                    let vol = wvar.sqrt() * TRADING_DAYS.sqrt();
                    RollingPoint { date: portfolio_curve[end].date, value: vol }
                })
                .collect()
        } else {
            Vec::new()
        };

        // --- Historical VaR / CVaR ---
        let (var_95, cvar_95, var_99, cvar_99) = quantile_risk(&port_returns);

        RiskMetrics {
            symbols: symbols.to_vec(),
            correlation,
            covariance,
            asset_volatility,
            asset_beta,
            contribution_to_risk,
            rolling_volatility,
            var_95,
            cvar_95,
            var_99,
            cvar_99,
        }
    }
}

/// Historical VaR and CVaR at 95% and 99% confidence.
/// Returns (var_95, cvar_95, var_99, cvar_99).
/// Both VaR and CVaR are negative for loss-making tail days.
fn quantile_risk(returns: &[f64]) -> (f64, f64, f64, f64) {
    if returns.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mut sorted = returns.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();

    let idx_95 = ((0.05 * n as f64).floor() as usize).min(n - 1);
    let var_95 = sorted[idx_95];
    let cvar_95 = sorted[..=idx_95].iter().sum::<f64>() / (idx_95 + 1) as f64;

    let idx_99 = ((0.01 * n as f64).floor() as usize).min(n - 1);
    let var_99 = sorted[idx_99];
    let cvar_99 = sorted[..=idx_99].iter().sum::<f64>() / (idx_99 + 1) as f64;

    (var_95, cvar_95, var_99, cvar_99)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::EquityPoint;

    fn make_curve(returns: &[f64]) -> Vec<EquityPoint> {
        let base = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let mut nav = 1000.0;
        let mut curve = vec![EquityPoint { date: base, nav }];
        for (i, &r) in returns.iter().enumerate() {
            nav *= 1.0 + r;
            let date = base + chrono::Duration::days(i as i64 + 1);
            curve.push(EquityPoint { date, nav });
        }
        curve
    }

    fn mk(
        syms: &[&str],
        asset_rets: Vec<Vec<f64>>,
        weights: &[f64],
        port_rets: &[f64],
    ) -> RiskMetrics {
        let symbols: Vec<String> = syms.iter().map(|s| s.to_string()).collect();
        let curve = make_curve(port_rets);
        RiskMetrics::compute(&symbols, &asset_rets, weights, &curve)
    }

    #[test]
    fn identical_series_correlate_to_one() {
        let r = vec![0.01, -0.02, 0.03, 0.01, -0.01];
        let rm = mk(&["A", "B"], vec![r.clone(), r.clone()], &[0.5, 0.5], &r);
        assert!(
            (rm.correlation[0][1] - 1.0).abs() < 1e-9,
            "corr[0][1] = {}",
            rm.correlation[0][1]
        );
    }

    #[test]
    fn anti_correlated_series_correlate_to_minus_one() {
        let r = vec![0.01, -0.02, 0.03, 0.01, -0.01];
        let neg: Vec<f64> = r.iter().map(|x| -x).collect();
        let rm = mk(&["A", "B"], vec![r.clone(), neg], &[0.5, 0.5], &r);
        assert!(
            (rm.correlation[0][1] + 1.0).abs() < 1e-9,
            "corr[0][1] = {}",
            rm.correlation[0][1]
        );
    }

    #[test]
    fn covariance_matrix_is_symmetric() {
        let r1 = vec![0.01, -0.02, 0.03, 0.00, -0.01];
        let r2 = vec![-0.01, 0.02, 0.01, -0.02, 0.03];
        let rm = mk(&["A", "B"], vec![r1, r2], &[0.5, 0.5], &[0.0; 5]);
        let eps = 1e-12;
        assert!((rm.covariance[0][1] - rm.covariance[1][0]).abs() < eps);
        assert!((rm.covariance[0][0] - rm.covariance[0][0]).abs() < eps); // trivially symmetric diagonal
    }

    #[test]
    fn contribution_to_risk_sums_to_one() {
        let r1 = vec![0.01, -0.02, 0.03, 0.01, -0.01];
        let r2 = vec![-0.01, 0.01, -0.02, 0.02, 0.00];
        let rm = mk(&["A", "B"], vec![r1, r2], &[0.6, 0.4], &[0.0; 5]);
        let sum: f64 = rm.contribution_to_risk.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-9,
            "CTR sum = {sum}"
        );
    }

    #[test]
    fn var_and_cvar_on_known_returns() {
        // 20 returns: 18 × +0.01, −0.05, −0.02.
        // sorted: [−0.05, −0.02, 0.01, …]
        // idx_95 = floor(0.05 × 20) = 1 → var_95 = −0.02
        // cvar_95 = mean(−0.05, −0.02) = −0.035
        // idx_99 = floor(0.01 × 20) = 0 → var_99 = −0.05
        // cvar_99 = −0.05
        let mut rets = vec![0.01_f64; 18];
        rets.push(-0.05);
        rets.push(-0.02);

        let r_asset = rets.clone();
        let rm = mk(&["A"], vec![r_asset], &[1.0], &rets);

        assert!((rm.var_95 - (-0.02)).abs() < 1e-9, "var_95 = {}", rm.var_95);
        assert!((rm.cvar_95 - (-0.035)).abs() < 1e-9, "cvar_95 = {}", rm.cvar_95);
        assert!((rm.var_99 - (-0.05)).abs() < 1e-9, "var_99 = {}", rm.var_99);
        assert!((rm.cvar_99 - (-0.05)).abs() < 1e-9, "cvar_99 = {}", rm.cvar_99);
    }

    #[test]
    fn cvar_is_at_most_var() {
        let rets: Vec<f64> = (-10_i32..=10).map(|i| i as f64 * 0.01).collect();
        let rm = mk(&["A"], vec![rets.clone()], &[1.0], &rets);
        assert!(rm.cvar_95 <= rm.var_95 + 1e-12, "CVaR-95 should be ≤ VaR-95");
        assert!(rm.cvar_99 <= rm.var_99 + 1e-12, "CVaR-99 should be ≤ VaR-99");
    }

    #[test]
    fn rolling_volatility_length_and_window() {
        // 25 port returns → curve of 26 bars.
        // Rolling points = 25 − 21 + 1 = 5.
        let rets: Vec<f64> = (0..25).map(|i| if i % 3 == 0 { -0.01 } else { 0.01 }).collect();
        let rm = mk(&["A"], vec![rets.clone()], &[1.0], &rets);
        assert_eq!(rm.rolling_volatility.len(), 25 - ROLLING_WINDOW + 1);
        for rp in &rm.rolling_volatility {
            assert!(rp.value.is_finite(), "vol = {}", rp.value);
        }
    }

    #[test]
    fn single_asset_has_unit_correlation_and_full_contribution() {
        let r = vec![0.01, -0.02, 0.03, 0.00, -0.01];
        let rm = mk(&["A"], vec![r.clone()], &[1.0], &r);
        assert!((rm.correlation[0][0] - 1.0).abs() < 1e-9);
        assert!((rm.contribution_to_risk[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn degenerate_short_series_returns_zeros() {
        // Only 1 return point → not enough for any computation.
        let r = vec![0.01];
        let symbols = vec!["A".to_string()];
        let curve = make_curve(&r);
        let rm = RiskMetrics::compute(&symbols, &[r], &[1.0], &curve);
        assert_eq!(rm.correlation, vec![vec![0.0]]);
        assert_eq!(rm.asset_volatility, vec![0.0]);
        assert!(rm.rolling_volatility.is_empty());
        assert_eq!(rm.var_95, 0.0);
    }
}
