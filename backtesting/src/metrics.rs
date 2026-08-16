use serde::Serialize;
use crate::bootstrap::{bootstrap_curve, BootstrapConfig, MetricUncertainty};
use crate::portfolio::{EquityPoint, TradeRecord};
use crate::stats::TRADING_DAYS;

/// Buy-and-hold benchmark: total return and CAGR for holding `initial_cash`
/// worth of the asset from `first_price` to `last_price` over `n_bars` trading days.
#[derive(Debug, Serialize, Clone)]
pub struct Benchmark {
    pub total_return: f64,
    pub cagr: f64,
}

impl Benchmark {
    pub fn compute(initial_cash: f64, first_price: f64, last_price: f64, n_bars: usize) -> Self {
        if first_price <= 0.0 || n_bars < 2 {
            return Self { total_return: 0.0, cagr: 0.0 };
        }
        let shares = (initial_cash / first_price) as u32;
        let cash_rem = initial_cash - shares as f64 * first_price;
        let end_nav = shares as f64 * last_price + cash_rem;
        let total_return = end_nav / initial_cash - 1.0;
        // `n_bars` observations span `n_bars - 1` return periods. Must match
        // `Metrics::compute`, since the two CAGRs are displayed side by side as
        // strategy vs buy-and-hold.
        let cagr = (end_nav / initial_cash).powf(TRADING_DAYS / (n_bars - 1) as f64) - 1.0;
        Self { total_return, cagr }
    }
}

/// Aggregate performance statistics derived from an equity curve and trade log.
#[derive(Debug, Serialize, Clone)]
pub struct Metrics {
    /// `last_nav / first_nav − 1`.
    pub total_return: f64,
    /// Compound annual growth rate, annualizing the `n − 1` return periods of
    /// an `n`-point curve at 252 periods/year.
    ///
    /// **Assumes daily bars.** The factor comes from the observation count, not
    /// from elapsed calendar time, so weekly or monthly candles are annualized
    /// as though they were daily and the result is wrong by the frequency
    /// ratio. Deriving the exponent from the curve's date span would be
    /// frequency-agnostic; it is not done here because [`Benchmark`] receives
    /// only a bar count and the two must agree.
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
    /// Bootstrap uncertainty around the path metrics, when a caller asked for it.
    ///
    /// `None` on the sweep path, and it must stay that way: an interval per grid
    /// cell would cost cells × resamples, and a grid of independent intervals
    /// invites reading a selected maximum as if the selection were free — which
    /// is the error the Deflated Sharpe Ratio exists to correct, not something an
    /// interval can fix.
    ///
    /// Skipped entirely when absent, so responses that do not request it are
    /// byte-identical to those produced before this field existed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncertainty: Option<MetricUncertainty>,
}

impl Metrics {
    /// Compute all metrics. Returns zeroed struct if the curve has fewer than 2 points.
    pub fn compute(curve: &[EquityPoint], trades: &[TradeRecord]) -> Self {
        let zero = Self {
            total_return: 0.0, cagr: 0.0, annualized_volatility: 0.0,
            max_drawdown: 0.0, sharpe_ratio: 0.0, sortino_ratio: 0.0,
            win_rate: None, trade_count: trades.len(), uncertainty: None,
        };
        if curve.len() < 2 { return zero; }

        let first_nav = curve[0].nav;
        let last_nav = curve[curve.len() - 1].nav;
        let n = curve.len();
        let total_return = last_nav / first_nav - 1.0;
        // An `n`-point curve spans `n - 1` return periods, not `n`. The
        // difference is negligible over years and material over a walk-forward
        // fold, where `Metrics::cagr` can also be the ranking metric.
        let periods = (n - 1) as f64;
        let cagr = (last_nav / first_nav).powf(TRADING_DAYS / periods) - 1.0;

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

        let annualized_volatility = std_dev * TRADING_DAYS.sqrt();
        let sharpe_ratio = if std_dev == 0.0 { 0.0 } else { mean / std_dev * TRADING_DAYS.sqrt() };

        // Sortino: downside deviation (returns below 0)
        let downside_var = returns.iter()
            .map(|&r| if r < 0.0 { r * r } else { 0.0 })
            .sum::<f64>() / nr;
        let downside_dev = downside_var.sqrt();
        let sortino_ratio = if downside_dev == 0.0 { 0.0 } else { mean / downside_dev * TRADING_DAYS.sqrt() };

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
            // Populated only by `with_uncertainty`, never here: `compute` runs
            // once per sweep grid cell and must stay cheap.
            uncertainty: None,
        }
    }

    /// Attach bootstrap intervals, computed from the same curve the metrics came
    /// from.
    ///
    /// Returns `self` untouched when the config is disabled or the series is too
    /// short to resample. An interval is an enrichment, never a reason for a
    /// backtest to fail — a 6-bar fold should still report its metrics.
    pub fn with_uncertainty(mut self, curve: &[EquityPoint], cfg: &BootstrapConfig) -> Self {
        if !cfg.enabled {
            return self;
        }
        self.uncertainty = bootstrap_curve(curve, cfg).ok();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate};

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
    fn metrics_are_invariant_to_the_curve_dates() {
        // `bootstrap::bootstrap_returns` synthesises dates for a bare return
        // series, which is only sound because nothing here reads them. The `cagr`
        // rustdoc contemplates deriving the annualisation from the date span one
        // day; if that happens, this test fails and names why rather than letting
        // the pooled out-of-sample interval quietly go wrong.
        let navs = [10_000.0, 10_500.0, 10_200.0, 11_000.0];
        let near: Vec<EquityPoint> = navs
            .iter()
            .enumerate()
            .map(|(i, &nav)| ep_seq(i, nav))
            .collect();
        // Same NAVs, dates a decade later and a week apart.
        let far: Vec<EquityPoint> = navs
            .iter()
            .enumerate()
            .map(|(i, &nav)| EquityPoint {
                date: NaiveDate::from_ymd_opt(2034, 6, 1).unwrap() + Duration::weeks(i as i64),
                nav,
            })
            .collect();

        let a = Metrics::compute(&near, &[]);
        let b = Metrics::compute(&far, &[]);
        assert_eq!(a.cagr, b.cagr, "cagr must not depend on the dates");
        assert_eq!(a.sharpe_ratio, b.sharpe_ratio);
        assert_eq!(a.max_drawdown, b.max_drawdown);
    }

    #[test]
    fn absent_uncertainty_serialises_to_the_pre_existing_json() {
        // The backward-compatibility contract. `api/main.py` has no response
        // models and returns the parsed blob verbatim, and every run is persisted
        // to the `runs` table, so a field appearing unbidden would change stored
        // history and the sweep payload alike.
        let curve = vec![ep(1, 10_000.0), ep(2, 10_500.0), ep(3, 10_200.0)];
        let json = serde_json::to_string(&Metrics::compute(&curve, &[])).unwrap();
        assert!(
            !json.contains("uncertainty"),
            "metrics without an interval must not mention it: {json}"
        );
    }

    #[test]
    fn with_uncertainty_attaches_an_interval_and_leaves_the_estimates_alone() {
        let curve: Vec<EquityPoint> = (0..60)
            .map(|i| ep_seq(i, 10_000.0 * (1.0 + 0.01 * ((i % 7) as f64 - 3.0))))
            .collect();
        let bare = Metrics::compute(&curve, &[]);
        let enriched = bare.clone().with_uncertainty(&curve, &BootstrapConfig::default());

        assert!(enriched.uncertainty.is_some());
        // Every point estimate must survive untouched — the interval is around
        // the reported number, not a replacement for it.
        assert_eq!(bare.total_return, enriched.total_return);
        assert_eq!(bare.cagr, enriched.cagr);
        assert_eq!(bare.sharpe_ratio, enriched.sharpe_ratio);
        assert_eq!(bare.sortino_ratio, enriched.sortino_ratio);
        assert_eq!(bare.max_drawdown, enriched.max_drawdown);
        assert_eq!(bare.annualized_volatility, enriched.annualized_volatility);
        assert_eq!(bare.trade_count, enriched.trade_count);
    }

    #[test]
    fn a_disabled_config_leaves_the_metrics_without_an_interval() {
        let curve: Vec<EquityPoint> = (0..60).map(|i| ep_seq(i, 10_000.0 + i as f64)).collect();
        let m = Metrics::compute(&curve, &[]).with_uncertainty(&curve, &BootstrapConfig::off());
        assert!(m.uncertainty.is_none());
    }

    #[test]
    fn a_curve_too_short_to_bootstrap_still_reports_its_metrics() {
        // An interval is an enrichment, not a precondition: a 4-bar fold must
        // still come back with a Sharpe rather than erroring out.
        let curve = vec![ep(1, 10_000.0), ep(2, 10_100.0), ep(3, 10_050.0)];
        let m = Metrics::compute(&curve, &[]).with_uncertainty(&curve, &BootstrapConfig::default());
        assert!(m.uncertainty.is_none());
        assert!(m.total_return != 0.0);
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
    fn benchmark_cagr_equals_total_return_over_one_year() {
        // 253 observations span 252 return periods = exactly one trading year,
        // so annualizing must be a no-op.
        let b = Benchmark::compute(10000.0, 100.0, 150.0, 253);
        assert!((b.cagr - b.total_return).abs() < 1e-6, "cagr = {}", b.cagr);
    }

    #[test]
    fn benchmark_single_bar_has_no_return_period() {
        // One observation spans zero return periods; annualizing is undefined.
        let b = Benchmark::compute(10000.0, 100.0, 150.0, 1);
        assert_eq!(b.cagr, 0.0);
    }

    /// An equity point `i` days after 2024-01-01, for curves longer than a month.
    fn ep_seq(i: usize, nav: f64) -> EquityPoint {
        EquityPoint {
            date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() + Duration::days(i as i64),
            nav,
        }
    }

    #[test]
    fn cagr_annualizes_by_return_periods_not_observations() {
        // 253 NAV observations = 252 daily returns = one trading year, so CAGR
        // must equal total return exactly. Annualizing by the observation count
        // instead uses 252/253 and undershoots.
        let curve: Vec<EquityPoint> = (0..253)
            .map(|i| ep_seq(i, 10_000.0 * 1.5_f64.powf(i as f64 / 252.0)))
            .collect();
        let m = Metrics::compute(&curve, &[]);
        assert!((m.total_return - 0.5).abs() < 1e-9, "total = {}", m.total_return);
        assert!((m.cagr - m.total_return).abs() < 1e-9, "cagr = {}", m.cagr);
    }

    #[test]
    fn cagr_matches_benchmark_convention_on_the_same_span() {
        // Metrics::cagr and Benchmark::cagr are shown side by side as strategy
        // vs buy-and-hold, so they must annualize identically over one span.
        let curve: Vec<EquityPoint> = (0..64)
            .map(|i| ep_seq(i, 10_000.0 * 1.5_f64.powf(i as f64 / 63.0)))
            .collect();
        let m = Metrics::compute(&curve, &[]);
        let b = Benchmark::compute(10_000.0, 100.0, 150.0, 64);
        assert!((m.cagr - b.cagr).abs() < 1e-9, "metrics {} vs benchmark {}", m.cagr, b.cagr);
    }
}
