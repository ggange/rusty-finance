//! Uncertainty on performance metrics.
//!
//! Every number in [`crate::metrics`] is a point estimate from one finite sample
//! of one price path. This module attaches an interval to the ones that are
//! functionals of the return series, by resampling that series.
//!
//! ## Why the stationary bootstrap
//!
//! An IID bootstrap draws each return independently, which destroys serial
//! dependence: autocorrelation and volatility clustering vanish from the
//! resamples. Because positive autocorrelation inflates the variance of the
//! sample mean, an IID resample *understates* the sampling variability of the
//! Sharpe ratio and the resulting interval comes out too tight — the exact
//! failure this module exists to stop committing.
//!
//! The stationary bootstrap (Politis & Romano 1994) resamples *blocks* of
//! consecutive returns whose lengths are geometric with mean `1/p`, wrapping at
//! the end of the series. Within a block the original ordering survives, and
//! with it the local autocorrelation and the local volatility regime. Because
//! the block length is random rather than fixed, the resampled series is itself
//! stationary — which is what distinguishes this from a moving-block bootstrap.
//!
//! ## What an interval here does *not* tell you
//!
//! These are **within-sample** intervals. They quantify sampling error in one
//! return series, given a strategy and its parameters. They do not correct for:
//!
//! - **Parameter selection.** A metric chosen as the best of an N-cell grid is a
//!   biased estimator, and no interval around it fixes that. That needs the
//!   Deflated Sharpe Ratio.
//! - **Repeated trials across assets or folds.** An interval per fold is not an
//!   interval for the average across folds, and folds over correlated assets in
//!   the same calendar window are not independent observations. See
//!   `docs/strategy-validation.md`.
//!
//! A narrow interval here is therefore necessary but nowhere near sufficient for
//! believing a result.

use serde::{Deserialize, Serialize};

use crate::metrics::Metrics;
use crate::portfolio::EquityPoint;
use crate::stats::{sort_ascending, tail_index, TRADING_DAYS};

/// Minimum returns needed before an interval means anything. Below this the
/// bootstrap is describing its own resampling noise more than the data.
const MIN_OBSERVATIONS: usize = 8;

// ─── Output ──────────────────────────────────────────────────────────────────

/// A two-sided percentile interval plus the bootstrap standard error.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Interval {
    /// Lower percentile endpoint at [`BootstrapConfig::confidence`].
    pub lo: f64,
    /// Upper percentile endpoint.
    pub hi: f64,
    /// Standard deviation of the metric across resamples. Reported alongside the
    /// endpoints because it is the quantity comparable to an analytic standard
    /// error such as [`sharpe_std_error_iid`].
    pub std_error: f64,
}

/// Bootstrap uncertainty for the metrics that are functionals of the return path.
///
/// `total_return` and `annualized_volatility` are absent because they are
/// monotone transforms of quantities already covered here; `win_rate` and
/// `trade_count` are absent because a resampled return path has no trades.
#[derive(Debug, Clone, Serialize)]
pub struct MetricUncertainty {
    /// Always `"stationary_bootstrap"`. Present so a stored run can be read back
    /// without guessing how its interval was produced.
    pub method: &'static str,
    /// Two-sided coverage, e.g. `0.95`.
    pub confidence: f64,
    pub resamples: usize,
    /// The mean block length actually used, after the automatic rule and clamp —
    /// not the requested value.
    pub mean_block: f64,
    pub seed: u64,
    /// Returns resampled, i.e. `curve.len() - 1`.
    pub observations: usize,
    pub sharpe_ratio: Interval,
    pub sortino_ratio: Interval,
    pub cagr: Interval,
    /// Standard error only, deliberately **not** an interval.
    ///
    /// Max drawdown depends on the ordering of the whole path, not on its
    /// short-range dependence. Blocks of a few bars preserve local structure but
    /// destroy the multi-month trends that produce deep drawdowns, so resampled
    /// drawdowns are systematically milder than the observed one. A percentile
    /// interval would frequently sit entirely *above* (less severe than) the
    /// point estimate, which reads as a bug and would mislead about direction.
    /// The spread is still worth reporting; the endpoints are not.
    pub max_drawdown_std_error: f64,
}

// ─── Configuration ───────────────────────────────────────────────────────────

/// Tuning for a bootstrap. The defaults are cheap enough to run on every
/// headline result without a caller asking for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// Off switch. `false` leaves the metrics' `uncertainty` field as `None`,
    /// which serializes to exactly the JSON produced before this module existed.
    pub enabled: bool,
    /// Resamples to draw. 1000 puts Monte-Carlo error on the 95 % endpoints at
    /// under one order statistic, well inside the estimator's own error, and
    /// costs about `resamples × n` float operations.
    pub resamples: usize,
    /// Expected block length in bars; `p = 1 / mean_block`.
    ///
    /// `None` applies `n^(1/3)` clamped to `[2, n/4]` — the standard order for
    /// the optimal block length, and unlike a fixed 20 it does not ask a 75-bar
    /// walk-forward fold to be described by four blocks.
    ///
    /// `Some(1.0)` degenerates to the IID bootstrap, which is what the
    /// stationary bootstrap is defined against and is useful as a control.
    pub mean_block: Option<f64>,
    /// Two-sided coverage, in `(0, 1)`.
    pub confidence: f64,
    /// PRNG seed. Fixed by default so a given run reproduces bit for bit;
    /// exposed so a caller can check an interval is not an artifact of one seed.
    pub seed: u64,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            resamples: 1000,
            // Automatic n^(1/3): the right block length depends on the series
            // length, and these run on everything from 75-bar folds to 1257-bar
            // catalogs.
            mean_block: None,
            confidence: 0.95,
            seed: 42,
        }
    }
}

impl BootstrapConfig {
    /// Disabled, for call paths that must stay cheap — notably the sweep grid,
    /// where a per-cell interval would cost cells × resamples and would invite
    /// reading a grid of independent intervals as if selection were free.
    pub fn off() -> Self {
        Self { enabled: false, ..Default::default() }
    }

    /// Effective mean block length for a series of `n` returns.
    fn effective_mean_block(&self, n: usize) -> f64 {
        let auto = (n as f64).cbrt();
        let upper = (n as f64 / 4.0).max(1.0);
        self.mean_block.unwrap_or(auto).clamp(1.0, upper.max(1.0))
    }

    fn validate(&self) -> Result<(), String> {
        if self.resamples == 0 {
            return Err("resamples must be > 0".to_string());
        }
        if !(self.confidence > 0.0 && self.confidence < 1.0) {
            return Err(format!("confidence must be in (0, 1), got {}", self.confidence));
        }
        if let Some(b) = self.mean_block {
            if !(b >= 1.0) {
                return Err(format!("mean_block must be >= 1.0, got {b}"));
            }
        }
        Ok(())
    }
}

// ─── PRNG ────────────────────────────────────────────────────────────────────

/// 64-bit LCG with MMIX constants, matching the generator style already used for
/// deterministic test series in [`crate::optimize`].
///
/// Hand-rolled to keep the core dependency-free. Output comes from the high 32
/// bits because an LCG's low bits have short periods.
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_mul(6364136223846793005).wrapping_add(1) }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }

    /// Uniform on `0..n`. Integer reduction rather than float scaling, so no
    /// rounding enters the reproducibility surface. The modulo bias is below one
    /// part in a million for any `n` a return series can reach.
    fn index(&mut self, n: usize) -> usize {
        self.next_u32() as usize % n
    }

    /// Uniform on `[0, 1)`.
    fn unit(&mut self) -> f64 {
        self.next_u32() as f64 / (u32::MAX as f64 + 1.0)
    }
}

// ─── Resampling ──────────────────────────────────────────────────────────────

/// Indices of one stationary-bootstrap resample of a series of length `n`.
///
/// Starts at a uniform position, then with probability `p` jumps to a new
/// uniform position and otherwise advances one step, **wrapping** at the end.
/// Block lengths are therefore geometric with mean `1/p`, and the wrap makes the
/// series circular so every observation is equally likely to appear — without
/// it, the tail of the series would be systematically under-sampled.
fn resample_indices(rng: &mut Lcg, n: usize, p: f64, out: &mut Vec<usize>) {
    out.clear();
    if n == 0 {
        return;
    }
    let mut i = rng.index(n);
    out.push(i);
    for _ in 1..n {
        i = if rng.unit() < p {
            rng.index(n) // start a new block
        } else {
            (i + 1) % n // continue this one, wrapping
        };
        out.push(i);
    }
}

/// Bootstrap the path metrics from an equity curve.
///
/// # Errors
/// Fewer than 8 returns, or an invalid config (see [`BootstrapConfig`]).
pub fn bootstrap_curve(
    curve: &[EquityPoint],
    cfg: &BootstrapConfig,
) -> Result<MetricUncertainty, String> {
    if curve.len() < 2 {
        return Err(format!(
            "need at least {} returns to bootstrap, got a {}-point curve",
            MIN_OBSERVATIONS,
            curve.len()
        ));
    }
    let returns: Vec<f64> = curve
        .windows(2)
        .map(|w| w[1].nav / w[0].nav - 1.0)
        .collect();
    bootstrap_impl(&returns, curve[0].nav, |i| curve[i].date, cfg)
}

/// Bootstrap the path metrics from a bare return series.
///
/// Used for the pooled out-of-sample path, where returns from separate folds are
/// concatenated and the NAV levels between them are not continuous.
///
/// # Errors
/// As [`bootstrap_curve`].
pub fn bootstrap_returns(
    returns: &[f64],
    cfg: &BootstrapConfig,
) -> Result<MetricUncertainty, String> {
    // Synthetic consecutive dates. `Metrics::compute` reads only NAV levels, a
    // property pinned by `metrics_are_invariant_to_the_curve_dates`.
    let base = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).expect("2000-01-01 is a valid date");
    bootstrap_impl(
        returns,
        1.0,
        |i| base + chrono::Duration::days(i as i64),
        cfg,
    )
}

fn bootstrap_impl(
    returns: &[f64],
    first_nav: f64,
    date_at: impl Fn(usize) -> chrono::NaiveDate,
    cfg: &BootstrapConfig,
) -> Result<MetricUncertainty, String> {
    cfg.validate()?;
    let n = returns.len();
    if n < MIN_OBSERVATIONS {
        return Err(format!(
            "need at least {MIN_OBSERVATIONS} returns to bootstrap, got {n}"
        ));
    }

    let mean_block = cfg.effective_mean_block(n);
    let p = 1.0 / mean_block;
    let mut rng = Lcg::new(cfg.seed);

    let mut sharpes = Vec::with_capacity(cfg.resamples);
    let mut sortinos = Vec::with_capacity(cfg.resamples);
    let mut cagrs = Vec::with_capacity(cfg.resamples);
    let mut drawdowns = Vec::with_capacity(cfg.resamples);

    // Buffers reused across resamples.
    let mut idx: Vec<usize> = Vec::with_capacity(n);
    let mut synth: Vec<EquityPoint> = (0..=n)
        .map(|i| EquityPoint { date: date_at(i), nav: first_nav })
        .collect();

    for _ in 0..cfg.resamples {
        resample_indices(&mut rng, n, p, &mut idx);

        // Compound the resampled returns into a curve, then hand it to the very
        // same estimator being bounded. Recomputing Sharpe here instead would
        // fork the zero-variance guard and the 252/(n-1) CAGR convention.
        let mut nav = first_nav;
        synth[0].nav = nav;
        for (k, &i) in idx.iter().enumerate() {
            nav *= 1.0 + returns[i];
            synth[k + 1].nav = nav;
        }

        let m = Metrics::compute(&synth, &[]);
        sharpes.push(m.sharpe_ratio);
        sortinos.push(m.sortino_ratio);
        cagrs.push(m.cagr);
        drawdowns.push(m.max_drawdown);
    }

    Ok(MetricUncertainty {
        method: "stationary_bootstrap",
        confidence: cfg.confidence,
        resamples: cfg.resamples,
        mean_block,
        seed: cfg.seed,
        observations: n,
        sharpe_ratio: interval(&mut sharpes, cfg.confidence),
        sortino_ratio: interval(&mut sortinos, cfg.confidence),
        cagr: interval(&mut cagrs, cfg.confidence),
        max_drawdown_std_error: std_error(&drawdowns),
    })
}

/// Percentile interval from a set of resampled metric values.
///
/// Both endpoints use the same [`tail_index`] convention, mirrored, so the
/// interval is widened at each end by the same rule rather than mixing a `ceil`
/// estimator on one side with a `floor` on the other.
fn interval(values: &mut [f64], confidence: f64) -> Interval {
    let se = std_error(values);
    sort_ascending(values);
    let m = values.len();
    let alpha = 1.0 - confidence;
    let lo_idx = tail_index(alpha / 2.0, m);
    Interval { lo: values[lo_idx], hi: values[m - 1 - lo_idx], std_error: se }
}

/// Population standard deviation, matching the denominator convention in
/// [`crate::metrics`].
fn std_error(values: &[f64]) -> f64 {
    let m = values.len() as f64;
    if m == 0.0 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / m;
    (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / m).sqrt()
}

// ─── Analytic oracle ─────────────────────────────────────────────────────────

/// Analytic standard error of the **annualized** Sharpe ratio *assuming
/// independent returns* (Jobson & Korkie 1981; Lo 2002 eq. 9).
///
/// ```text
/// SE(SR_annual) = √q · sqrt((1 + SR_p²/2) / n)
/// ```
///
/// where `SR_p = μ/σ` is the periodic Sharpe (rf = 0, population σ, matching
/// [`Metrics`]) and `q = 252`.
///
/// This is the oracle for [`bootstrap_curve`], and the `√q` is deliberate: the
/// engine annualizes naively, so this bounds the same quantity the bootstrap
/// resamples. Two independently derived estimators of one quantity should agree
/// on independent data — without such a check, a bootstrap that silently stopped
/// resampling would still return a plausible-looking number.
///
/// On *dependent* data the bootstrap should come out **wider** than this, since
/// the IID assumption here is exactly what fails. That gap is the value the
/// bootstrap adds, and it is asserted by
/// `the_stationary_interval_is_wider_than_the_iid_interval_on_an_autocorrelated_series`.
pub fn sharpe_std_error_iid(returns: &[f64]) -> f64 {
    let n = returns.len();
    if n < 2 {
        return 0.0;
    }
    let nf = n as f64;
    let mean = returns.iter().sum::<f64>() / nf;
    let var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / nf;
    if var == 0.0 {
        return 0.0;
    }
    let sr = mean / var.sqrt();
    TRADING_DAYS.sqrt() * ((1.0 + sr * sr / 2.0) / nf).sqrt()
}

/// Lo (2002) autocorrelation-corrected annualization factor `η(q)`, in place of
/// the naive `√q`.
///
/// ```text
/// η(q) = q / sqrt(q + 2·Σ_{k=1..min(q, max_lag)} (q − k)·ρ_k)     Lo eqs. 19-20
/// ```
///
/// Under serial independence `η(q) = q/√q = √q` and the correction vanishes.
/// Under positive autocorrelation the denominator grows and `η(q) < √q`, meaning
/// **the engine's annualized Sharpe is overstated** — a strategy whose daily
/// returns trend gets flattered by `√252`.
///
/// ## Why this is not the standard error
///
/// It is tempting to read `η` as an uncertainty correction. It is not, and the
/// distinction matters: `η` scales the annualized Sharpe *and* its standard error
/// by the same factor, so it cancels exactly in the t-statistic `SR/SE` and
/// changes no significance verdict. `η` corrects the **point estimate**;
/// widening the interval under dependence is the bootstrap's job, and
/// [`sharpe_std_error_iid`] is what it should exceed.
///
/// ## Choosing `max_lag`
///
/// Keep it small — 10 or so. Each `ρ_k` carries sampling error of order
/// `1/√n`, and the `(q − k)` weights are near `q` for small `k`, so summing all
/// `q − 1` lags accumulates noise faster than signal: on 4000 serially
/// independent returns, `max_lag = 251` returns `η/√q ≈ 1.19` where the truth is
/// `1.0`, while `max_lag = 10` gets within about 10 %.
pub fn lo_annualization_factor(returns: &[f64], q: usize, max_lag: usize) -> f64 {
    let n = returns.len();
    let qf = q as f64;
    if n < 2 || q == 0 {
        return qf.sqrt();
    }
    let nf = n as f64;
    let mean = returns.iter().sum::<f64>() / nf;
    let var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / nf;
    if var == 0.0 {
        return qf.sqrt();
    }

    let weighted_rho: f64 = (1..q.min(max_lag + 1))
        .filter(|&k| k < n)
        .map(|k| {
            let cov: f64 = (k..n)
                .map(|t| (returns[t] - mean) * (returns[t - k] - mean))
                .sum::<f64>()
                / nf;
            (q - k) as f64 * (cov / var)
        })
        .sum();

    let inner = qf + 2.0 * weighted_rho;
    // Strong negative autocorrelation can drive the variance sum non-positive,
    // which has no square root; fall back to the naive factor rather than NaN.
    if inner > 0.0 { qf / inner.sqrt() } else { qf.sqrt() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    /// Deterministic pseudo-random returns, same generator style as the test
    /// helper in `optimize.rs`.
    fn series(n: usize, daily_vol: f64, drift: f64, seed: u64) -> Vec<f64> {
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (0..n)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let u = ((state >> 32) as f64 / u32::MAX as f64) * 2.0 - 1.0;
                drift + u * daily_vol * 1.732
            })
            .collect()
    }

    /// AR(1) returns: `r_t = φ·r_{t-1} + ε_t`. Positive φ gives the serial
    /// dependence an IID bootstrap cannot see.
    fn ar1(n: usize, phi: f64, daily_vol: f64, seed: u64) -> Vec<f64> {
        let eps = series(n, daily_vol, 0.0002, seed);
        let mut out = Vec::with_capacity(n);
        let mut prev = 0.0;
        for e in eps {
            let r = phi * prev + e;
            out.push(r);
            prev = r;
        }
        out
    }

    fn curve_from(returns: &[f64]) -> Vec<EquityPoint> {
        let base = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let mut nav = 10_000.0;
        let mut out = vec![EquityPoint { date: base, nav }];
        for (i, r) in returns.iter().enumerate() {
            nav *= 1.0 + r;
            out.push(EquityPoint {
                date: base + chrono::Duration::days(i as i64 + 1),
                nav,
            });
        }
        out
    }

    fn cfg(overrides: impl Fn(&mut BootstrapConfig)) -> BootstrapConfig {
        let mut c = BootstrapConfig::default();
        overrides(&mut c);
        c
    }

    // ─── Determinism ─────────────────────────────────────────────────────────

    #[test]
    fn a_fixed_seed_reproduces_the_interval_bit_for_bit() {
        // Exact equality, as `solutions_are_deterministic` does for the solver:
        // a stored interval that moved between runs would make every published
        // number unreproducible.
        let c = curve_from(&series(300, 0.01, 0.0003, 11));
        let a = bootstrap_curve(&c, &BootstrapConfig::default()).unwrap();
        let b = bootstrap_curve(&c, &BootstrapConfig::default()).unwrap();
        assert_eq!(a.sharpe_ratio.lo, b.sharpe_ratio.lo);
        assert_eq!(a.sharpe_ratio.hi, b.sharpe_ratio.hi);
        assert_eq!(a.sharpe_ratio.std_error, b.sharpe_ratio.std_error);
        assert_eq!(a.cagr.lo, b.cagr.lo);
    }

    #[test]
    fn changing_the_seed_moves_the_endpoints() {
        // The complement of the test above: guards against a config field that
        // is silently ignored, which would make determinism vacuous.
        let c = curve_from(&series(300, 0.01, 0.0003, 12));
        let a = bootstrap_curve(&c, &cfg(|x| x.seed = 1)).unwrap();
        let b = bootstrap_curve(&c, &cfg(|x| x.seed = 2)).unwrap();
        assert_ne!(a.sharpe_ratio.lo, b.sharpe_ratio.lo);
    }

    // ─── Resampling mechanics ────────────────────────────────────────────────

    #[test]
    fn a_resample_has_the_same_length_as_the_original_series() {
        let mut rng = Lcg::new(7);
        let mut idx = Vec::new();
        resample_indices(&mut rng, 50, 0.1, &mut idx);
        assert_eq!(idx.len(), 50);
        assert!(idx.iter().all(|&i| i < 50));
    }

    #[test]
    fn a_mean_block_of_one_never_continues_a_block() {
        // p = 1 means every draw starts a new block, which is the IID bootstrap.
        // Any consecutive pair is then coincidence, not continuation — so assert
        // on the mechanism instead: with p = 1 the "continue" branch is dead.
        let mut rng = Lcg::new(7);
        let mut idx = Vec::new();
        resample_indices(&mut rng, 200, 1.0, &mut idx);
        let steps = idx.windows(2).filter(|w| w[1] == (w[0] + 1) % 200).count();
        // Chance alone gives ~1/200 per pair, so ~1 in 199. Allow generous slack.
        assert!(steps < 10, "p=1 should rarely step consecutively, got {steps}");
    }

    #[test]
    fn a_long_mean_block_mostly_continues_the_block() {
        let mut rng = Lcg::new(7);
        let mut idx = Vec::new();
        resample_indices(&mut rng, 200, 0.01, &mut idx); // mean block 100
        let steps = idx.windows(2).filter(|w| w[1] == (w[0] + 1) % 200).count();
        assert!(steps > 180, "mean block 100 should almost always step, got {steps}");
    }

    #[test]
    fn a_block_starting_at_the_last_return_wraps_to_the_first() {
        // The circular wrap is what keeps the tail of the series from being
        // under-sampled. Drive it directly: p ~ 0 so a block never restarts, and
        // whatever index we land on, the successor must be modular.
        let mut rng = Lcg::new(3);
        let mut idx = Vec::new();
        let n = 20;
        resample_indices(&mut rng, n, 0.0, &mut idx);
        for w in idx.windows(2) {
            assert_eq!(w[1], (w[0] + 1) % n, "block must advance modulo n");
        }
        assert!(idx.iter().any(|&i| i == 0), "a wrapping walk of length n visits 0");
    }

    // ─── The property that justifies the module ──────────────────────────────

    #[test]
    fn the_stationary_interval_is_wider_than_the_iid_interval_on_an_autocorrelated_series() {
        // AR(1) with φ = 0.6: the long-run variance of the mean is inflated by
        // (1+φ)/(1−φ) = 4, so theory predicts a standard-error ratio of √4 = 2.0.
        // Measured: 2.089. That agreement with a closed-form prediction is the
        // strongest evidence the block resampling is actually correct, not merely
        // wider. An IID bootstrap cannot see any of this and reports an interval
        // that is too tight — which is the whole reason for blocks.
        let returns = ar1(1000, 0.6, 0.01, 21);
        let c = curve_from(&returns);

        let stationary = bootstrap_curve(&c, &BootstrapConfig::default()).unwrap();
        let iid = bootstrap_curve(&c, &cfg(|x| x.mean_block = Some(1.0))).unwrap();

        let ratio = stationary.sharpe_ratio.std_error / iid.sharpe_ratio.std_error;
        assert!(
            ratio > 1.25,
            "stationary SE {} vs IID SE {} (ratio {ratio:.3}) — blocks are not capturing the dependence",
            stationary.sharpe_ratio.std_error,
            iid.sharpe_ratio.std_error
        );
    }

    #[test]
    fn the_stationary_and_iid_intervals_agree_on_an_independent_series() {
        // The contrastive partner of the test above. On serially independent
        // returns there is no dependence to capture, so the two must agree —
        // proving the widening above comes from the data, not from the block
        // machinery inflating everything it touches. Measured ratio: 1.002.
        let returns = series(1000, 0.01, 0.0003, 22);
        let c = curve_from(&returns);

        let stationary = bootstrap_curve(&c, &BootstrapConfig::default()).unwrap();
        let iid = bootstrap_curve(&c, &cfg(|x| x.mean_block = Some(1.0))).unwrap();

        let ratio = stationary.sharpe_ratio.std_error / iid.sharpe_ratio.std_error;
        assert!(
            (ratio - 1.0).abs() < 0.25,
            "on IID data the two should agree, got ratio {ratio:.3}"
        );
    }

    // ─── Agreement with the analytic oracle ──────────────────────────────────

    #[test]
    fn the_bootstrap_standard_error_matches_the_analytic_oracle_on_independent_returns() {
        // Two independently derived estimators of one quantity, on data long
        // enough for both to be near asymptotic. Nothing here is random — the
        // bootstrap is seeded and the series is deterministic — so the tolerance
        // covers estimator disagreement, not run-to-run noise. Observed ratio at
        // the time of writing: 1.036.
        let returns = series(2000, 0.012, 0.0004, 31);
        let c = curve_from(&returns);

        let boot = bootstrap_curve(&c, &BootstrapConfig::default())
            .unwrap()
            .sharpe_ratio
            .std_error;
        let oracle = sharpe_std_error_iid(&returns);

        let ratio = boot / oracle;
        assert!(
            (ratio - 1.0).abs() < 0.15,
            "bootstrap SE {boot:.4} vs analytic oracle {oracle:.4} (ratio {ratio:.3})"
        );
    }

    #[test]
    fn the_bootstrap_exceeds_the_iid_oracle_on_dependent_returns() {
        // The complement, and the reason the bootstrap earns its keep: where the
        // analytic formula's independence assumption fails, resampling blocks
        // sees uncertainty the formula cannot.
        let returns = ar1(1000, 0.6, 0.01, 34);
        let c = curve_from(&returns);

        let boot = bootstrap_curve(&c, &BootstrapConfig::default())
            .unwrap()
            .sharpe_ratio
            .std_error;
        let oracle = sharpe_std_error_iid(&returns);

        assert!(
            boot > oracle * 1.2,
            "bootstrap SE {boot:.4} should exceed the IID oracle {oracle:.4} under dependence"
        );
    }

    #[test]
    fn the_lo_factor_reduces_to_root_q_when_returns_are_serially_independent() {
        // With ρ_k ≈ 0, η(q) = q/√q = √252 = 15.874 and the correction vanishes.
        // max_lag is 10 deliberately: at 251 lags the estimator is noise-dominated
        // and returns η/√q ≈ 1.19 on this very series.
        let returns = series(4000, 0.01, 0.0, 32);
        let eta = lo_annualization_factor(&returns, TRADING_DAYS as usize, 10);
        let ratio = eta / TRADING_DAYS.sqrt();
        assert!(
            (ratio - 1.0).abs() < 0.15,
            "on independent returns η should be √252 = 15.874, got {eta:.3} (ratio {ratio:.3})"
        );
    }

    #[test]
    fn positive_autocorrelation_shrinks_the_annualisation_factor_below_root_q() {
        // η(q) < √q means the engine's √252 annualization *overstates* the Sharpe
        // of a trending return series. At φ = 0.6 the factor roughly halves.
        let eta = lo_annualization_factor(&ar1(2000, 0.6, 0.01, 33), TRADING_DAYS as usize, 10);
        let naive = TRADING_DAYS.sqrt();
        assert!(
            eta < naive * 0.7,
            "η {eta:.3} should fall well below the naive {naive:.3} under φ=0.6"
        );
    }

    #[test]
    fn the_lo_factor_cancels_in_the_t_statistic() {
        // The property that stops η being mistaken for an uncertainty correction:
        // it scales the annualized Sharpe and its standard error identically, so
        // SR/SE is unchanged and no significance verdict moves. Correcting the
        // point estimate and widening the interval are different jobs.
        let returns = ar1(2000, 0.6, 0.01, 35);
        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        let sr_p = mean / var.sqrt();
        let se_p = ((1.0 + sr_p * sr_p / 2.0) / n).sqrt();

        let eta = lo_annualization_factor(&returns, TRADING_DAYS as usize, 10);
        let t_naive = (TRADING_DAYS.sqrt() * sr_p) / (TRADING_DAYS.sqrt() * se_p);
        let t_lo = (eta * sr_p) / (eta * se_p);

        assert!(
            (t_naive - t_lo).abs() < 1e-9,
            "η must cancel: naive t {t_naive:.6} vs Lo-adjusted t {t_lo:.6}"
        );
    }

    // ─── Interval shape ──────────────────────────────────────────────────────

    #[test]
    fn the_interval_endpoints_are_the_twenty_fifth_and_nine_hundred_seventy_sixth_of_a_thousand_resamples()
    {
        // tail_index(0.025, 1000) = ceil(25) - 1 = 24, so lo = v[24] and, mirrored,
        // hi = v[1000 - 1 - 24] = v[975]. Same convention as VaR, applied at both
        // ends so each is the interval-widening order statistic.
        let mut values: Vec<f64> = (0..1000).map(|i| i as f64).collect();
        let iv = interval(&mut values, 0.95);
        assert_eq!(iv.lo, 24.0);
        assert_eq!(iv.hi, 975.0);
    }

    #[test]
    fn a_ninety_nine_percent_interval_contains_a_ninety_five_percent_interval() {
        let c = curve_from(&series(400, 0.01, 0.0003, 41));
        let narrow = bootstrap_curve(&c, &cfg(|x| x.confidence = 0.95)).unwrap();
        let wide = bootstrap_curve(&c, &cfg(|x| x.confidence = 0.99)).unwrap();
        assert!(wide.sharpe_ratio.lo <= narrow.sharpe_ratio.lo);
        assert!(wide.sharpe_ratio.hi >= narrow.sharpe_ratio.hi);
    }

    #[test]
    fn the_interval_brackets_its_own_standard_error() {
        let c = curve_from(&series(400, 0.01, 0.0003, 42));
        let u = bootstrap_curve(&c, &BootstrapConfig::default()).unwrap();
        assert!(u.sharpe_ratio.hi > u.sharpe_ratio.lo);
        assert!(u.sharpe_ratio.std_error > 0.0);
    }

    #[test]
    fn a_flat_equity_curve_yields_a_degenerate_zero_interval() {
        // Every resample of an all-zero return series is all-zero, and the
        // zero-variance guard in Metrics::compute returns 0.0 rather than NaN.
        let c = curve_from(&vec![0.0; 50]);
        let u = bootstrap_curve(&c, &BootstrapConfig::default()).unwrap();
        assert_eq!(u.sharpe_ratio.lo, 0.0);
        assert_eq!(u.sharpe_ratio.hi, 0.0);
        assert_eq!(u.sharpe_ratio.std_error, 0.0);
    }

    #[test]
    fn the_effective_mean_block_grows_with_the_series_but_stays_usable_on_a_short_fold() {
        let c = BootstrapConfig::default();
        // 75-bar walk-forward fold: n^(1/3) ≈ 4.2, so ~18 blocks — thin but
        // meaningful. A fixed 20 would have given fewer than 4.
        let short = c.effective_mean_block(75);
        assert!((4.0..5.0).contains(&short), "75-bar block length was {short}");
        // Full catalog: 1256 returns → ~10.8.
        let long = c.effective_mean_block(1256);
        assert!((10.0..12.0).contains(&long), "1256-bar block length was {long}");
    }

    // ─── Rejections ──────────────────────────────────────────────────────────

    #[test]
    fn a_curve_too_short_to_resample_is_rejected() {
        let c = curve_from(&vec![0.01; 3]);
        let err = bootstrap_curve(&c, &BootstrapConfig::default()).unwrap_err();
        assert!(err.contains("at least 8"), "error should name the minimum: {err}");
    }

    #[test]
    fn zero_resamples_is_rejected_rather_than_producing_an_empty_interval() {
        let c = curve_from(&series(50, 0.01, 0.0, 51));
        assert!(bootstrap_curve(&c, &cfg(|x| x.resamples = 0)).is_err());
    }

    #[test]
    fn a_confidence_outside_the_unit_interval_is_rejected() {
        let c = curve_from(&series(50, 0.01, 0.0, 52));
        assert!(bootstrap_curve(&c, &cfg(|x| x.confidence = 1.0)).is_err());
        assert!(bootstrap_curve(&c, &cfg(|x| x.confidence = 0.0)).is_err());
    }

    // ─── Return-series entry point ───────────────────────────────────────────

    #[test]
    fn bootstrapping_a_bare_return_series_matches_bootstrapping_its_curve() {
        // The pooled out-of-sample path uses bootstrap_returns; it must agree
        // with the curve path on the same returns, since NAV level and dates are
        // irrelevant to every metric bounded here.
        let returns = series(300, 0.01, 0.0003, 61);
        let from_returns = bootstrap_returns(&returns, &BootstrapConfig::default()).unwrap();
        let from_curve = bootstrap_curve(&curve_from(&returns), &BootstrapConfig::default()).unwrap();
        assert!(
            (from_returns.sharpe_ratio.lo - from_curve.sharpe_ratio.lo).abs() < 1e-9,
            "returns {} vs curve {}",
            from_returns.sharpe_ratio.lo,
            from_curve.sharpe_ratio.lo
        );
    }
}
