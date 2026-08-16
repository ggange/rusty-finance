//! Portfolio weight optimization.
//!
//! Given a set of per-asset return series, solve for capital weights under a
//! chosen objective. Every solver here is **long-only and fully invested**:
//! weights are non-negative and sum to 1, optionally capped per position. That
//! is not a simplification for its own sake — the execution engine
//! ([`crate::portfolio::Portfolio`]) cannot express a short, so a solver that
//! returned negative weights would produce a portfolio the backtest cannot run.
//!
//! ## On trusting these numbers
//!
//! Sample covariance estimated from a finite window is noisy, and mean returns
//! are noisier still. Unconstrained mean-variance optimizers are notorious for
//! concentrating into whatever asset happened to win in-sample. Two defences
//! are built in and on by default:
//!
//! - **Shrinkage** ([`OptimizerConfig::shrinkage`]) pulls the covariance matrix
//!   toward its diagonal, which damps unstable off-diagonal estimates.
//! - **Position caps** ([`OptimizerConfig::max_weight`]) bound concentration
//!   directly.
//!
//! Neither makes an in-sample optimum predictive. Treat a solved weight vector
//! the way the project treats a backtest: evidence, not truth. [`Objective::MaxSharpe`]
//! is the most exposed to estimation error because it depends on mean returns;
//! [`Objective::RiskParity`] and [`Objective::MinVariance`] depend only on
//! covariance and are correspondingly more stable.

use serde::{Deserialize, Serialize};

/// Trading days per year, matching [`crate::metrics`] and [`crate::risk`].
const TRADING_DAYS: f64 = 252.0;

/// What the optimizer is solving for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    /// 1/n. The honest baseline: no estimation, and hard to beat out-of-sample.
    EqualWeight,
    /// Weight inversely proportional to volatility. Ignores correlation.
    InverseVolatility,
    /// Minimize portfolio variance. Depends on covariance only, not on means.
    MinVariance,
    /// Equalize each asset's contribution to total portfolio risk.
    RiskParity,
    /// Maximize the (in-sample) Sharpe ratio. Depends on mean returns, so this
    /// is the objective most vulnerable to estimation error.
    MaxSharpe,
}

impl Objective {
    /// Whether this objective consumes mean returns. Objectives that do not are
    /// materially more stable out-of-sample.
    pub fn uses_expected_returns(&self) -> bool {
        matches!(self, Objective::MaxSharpe)
    }
}

/// Tuning for a weight solve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerConfig {
    pub objective: Objective,
    /// Covariance shrinkage toward the diagonal, in `[0, 1]`. 0 = raw sample
    /// covariance, 1 = diagonal only (correlations discarded).
    pub shrinkage: f64,
    /// Optional per-position cap in `(0, 1]`. `None` means uncapped.
    pub max_weight: Option<f64>,
    /// Iteration budget for the iterative solvers.
    pub max_iter: usize,
    /// Convergence tolerance on the weight vector's max element change.
    pub tol: f64,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            objective: Objective::RiskParity,
            // A modest default: enough to stabilize a short window without
            // discarding genuine correlation structure.
            shrinkage: 0.2,
            max_weight: None,
            max_iter: 500,
            tol: 1e-9,
        }
    }
}

impl OptimizerConfig {
    pub fn new(objective: Objective) -> Self {
        Self { objective, ..Default::default() }
    }
}

/// A solved weight vector plus the diagnostics that explain it.
#[derive(Debug, Clone, Serialize)]
pub struct WeightSolution {
    /// Non-negative, sums to 1 (all-zero only when there are no assets).
    pub weights: Vec<f64>,
    /// Annualized portfolio volatility implied by the estimation window.
    pub expected_volatility: f64,
    /// Annualized portfolio return implied by the estimation window.
    pub expected_return: f64,
    /// Each asset's share of total portfolio risk. Sums to 1. For
    /// [`Objective::RiskParity`] these should all be ≈ 1/n.
    pub risk_contribution: Vec<f64>,
    /// Iterations actually used; `0` for the closed-form objectives.
    pub iterations: usize,
    /// True when an iterative solver hit `max_iter` without meeting `tol`.
    pub hit_iteration_limit: bool,
}

impl WeightSolution {
    fn degenerate(n: usize) -> Self {
        Self {
            weights: if n == 0 { Vec::new() } else { vec![1.0 / n as f64; n] },
            expected_volatility: 0.0,
            expected_return: 0.0,
            risk_contribution: if n == 0 { Vec::new() } else { vec![1.0 / n as f64; n] },
            iterations: 0,
            hit_iteration_limit: false,
        }
    }
}

// ─── Estimation ──────────────────────────────────────────────────────────────

/// Annualized mean return per asset, from a daily return series.
pub fn mean_returns(returns: &[Vec<f64>]) -> Vec<f64> {
    returns
        .iter()
        .map(|r| {
            if r.is_empty() {
                0.0
            } else {
                r.iter().sum::<f64>() / r.len() as f64 * TRADING_DAYS
            }
        })
        .collect()
}

/// Annualized population covariance matrix from **date-aligned** daily return
/// series.
///
/// Alignment is the caller's responsibility, and it matters: index `k` must be
/// the same calendar date in every series, or the result is a covariance between
/// different periods, which measures nothing. This function has no dates and so
/// cannot check or repair that — it can only see lengths.
///
/// Unequal lengths are not rejected — this is reachable from the FFI boundary,
/// where a panic is worse than a defensible guess — but they are a caller bug.
/// The fallback keeps the most recent `t` observations of each series: aligning
/// on the *end* assumes the series share a last date, which is far more often
/// true of price history than sharing a first one. Head alignment, the previous
/// behaviour, assumes a shared *start* and so pairs a long history's oldest bars
/// with a short history's newest — the worst of the available guesses. Callers
/// with dates in hand should align before calling and not rely on either.
pub fn covariance(returns: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = returns.len();
    if n == 0 {
        return Vec::new();
    }
    let t = returns.iter().map(|r| r.len()).min().unwrap_or(0);
    if t < 2 {
        return vec![vec![0.0; n]; n];
    }

    // Keep the trailing `t` observations of each series — a no-op when lengths
    // are equal, as an aligned caller guarantees.
    let aligned: Vec<&[f64]> = returns.iter().map(|r| &r[r.len() - t..]).collect();

    let means: Vec<f64> = aligned.iter().map(|r| r.iter().sum::<f64>() / t as f64).collect();

    let mut cov = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in i..n {
            let c = (0..t)
                .map(|k| (aligned[i][k] - means[i]) * (aligned[j][k] - means[j]))
                .sum::<f64>()
                / t as f64
                * TRADING_DAYS;
            cov[i][j] = c;
            cov[j][i] = c;
        }
    }
    cov
}

/// Shrink a covariance matrix toward its diagonal.
///
/// `Σ' = (1 − λ)·Σ + λ·diag(Σ)`. Variances are preserved exactly; only the
/// off-diagonal covariances are damped. λ is clamped to `[0, 1]`.
pub fn shrink_covariance(cov: &[Vec<f64>], lambda: f64) -> Vec<Vec<f64>> {
    let l = lambda.clamp(0.0, 1.0);
    cov.iter()
        .enumerate()
        .map(|(i, row)| {
            row.iter()
                .enumerate()
                .map(|(j, &c)| if i == j { c } else { c * (1.0 - l) })
                .collect()
        })
        .collect()
}

// ─── Linear algebra helpers ──────────────────────────────────────────────────

/// `Σw`.
fn mat_vec(cov: &[Vec<f64>], w: &[f64]) -> Vec<f64> {
    cov.iter().map(|row| row.iter().zip(w).map(|(a, b)| a * b).sum()).collect()
}

/// `wᵀΣw`, floored at 0 to absorb float noise on a near-singular matrix.
fn quad_form(cov: &[Vec<f64>], w: &[f64]) -> f64 {
    mat_vec(cov, w).iter().zip(w).map(|(a, b)| a * b).sum::<f64>().max(0.0)
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Euclidean projection onto the probability simplex `{w : Σw = 1, w ≥ 0}`.
///
/// The classic sort-based algorithm (Duchi et al. 2008). Used as the feasibility
/// step of projected gradient descent, so every iterate stays a valid long-only
/// fully-invested portfolio.
pub fn project_to_simplex(v: &[f64]) -> Vec<f64> {
    let n = v.len();
    if n == 0 {
        return Vec::new();
    }
    let mut sorted: Vec<f64> = v.to_vec();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let mut cumulative = 0.0;
    let mut rho = 0usize;
    let mut theta = 0.0;
    for (i, &s) in sorted.iter().enumerate() {
        cumulative += s;
        let t = (cumulative - 1.0) / (i as f64 + 1.0);
        if s - t > 0.0 {
            rho = i + 1;
            theta = t;
        }
    }
    if rho == 0 {
        // Every candidate was dominated (can happen with extreme inputs);
        // equal weight is the safe feasible fallback.
        return vec![1.0 / n as f64; n];
    }
    v.iter().map(|&x| (x - theta).max(0.0)).collect()
}

/// Project onto the simplex, then enforce a per-position cap by clipping the
/// over-cap names and redistributing their excess among the rest.
///
/// A cap below `1/n` is infeasible (the weights could not then sum to 1), so it
/// is raised to `1/n`, which yields equal weight.
fn project_capped(v: &[f64], max_weight: Option<f64>) -> Vec<f64> {
    let n = v.len();
    let mut w = project_to_simplex(v);
    let Some(cap_raw) = max_weight else { return w };
    if n == 0 {
        return w;
    }
    let cap = cap_raw.max(1.0 / n as f64).min(1.0);

    // Iteratively clip and redistribute. Each pass fixes at least one name at
    // the cap, so this terminates in at most n passes.
    for _ in 0..n {
        let over: Vec<usize> = (0..n).filter(|&i| w[i] > cap + 1e-12).collect();
        if over.is_empty() {
            break;
        }
        let mut excess = 0.0;
        for &i in &over {
            excess += w[i] - cap;
            w[i] = cap;
        }
        let free: Vec<usize> = (0..n).filter(|&i| w[i] < cap - 1e-12).collect();
        if free.is_empty() {
            break;
        }
        let free_total: f64 = free.iter().map(|&i| w[i]).sum();
        for &i in &free {
            // Redistribute proportionally where possible, evenly when the free
            // names are all at zero.
            w[i] += if free_total > 0.0 {
                excess * w[i] / free_total
            } else {
                excess / free.len() as f64
            };
        }
    }
    w
}

// ─── Solvers ─────────────────────────────────────────────────────────────────

/// Per-asset share of total portfolio risk: `w_i·(Σw)_i / (wᵀΣw)`. Sums to 1.
pub fn risk_contribution(cov: &[Vec<f64>], w: &[f64]) -> Vec<f64> {
    let var = quad_form(cov, w);
    if var <= 0.0 {
        let n = w.len();
        return if n == 0 { Vec::new() } else { vec![1.0 / n as f64; n] };
    }
    let sw = mat_vec(cov, w);
    w.iter().zip(&sw).map(|(wi, si)| wi * si / var).collect()
}

fn inverse_volatility(cov: &[Vec<f64>]) -> Vec<f64> {
    let inv: Vec<f64> = (0..cov.len())
        .map(|i| {
            let var = cov[i][i];
            if var > 0.0 { 1.0 / var.sqrt() } else { 0.0 }
        })
        .collect();
    let sum: f64 = inv.iter().sum();
    if sum > 0.0 {
        inv.iter().map(|x| x / sum).collect()
    } else {
        vec![1.0 / cov.len() as f64; cov.len()]
    }
}

/// Risk parity by multiplicative fixed-point iteration.
///
/// Repeatedly nudges each weight by the ratio of its target risk share to its
/// actual share, then renormalizes. Converges reliably for positive-definite
/// covariance and needs no matrix inverse — which matters here because this runs
/// once per rebalance date in a dynamic backtest.
fn solve_risk_parity(cov: &[Vec<f64>], cfg: &OptimizerConfig) -> (Vec<f64>, usize, bool) {
    let n = cov.len();
    let target = 1.0 / n as f64;
    let mut w = vec![target; n];
    let mut iterations = 0;
    let mut converged = false;

    for it in 1..=cfg.max_iter {
        iterations = it;
        let rc = risk_contribution(cov, &w);
        let mut next: Vec<f64> = w
            .iter()
            .zip(&rc)
            .map(|(wi, rci)| {
                // Damped update (sqrt) — the raw ratio overshoots and oscillates.
                if *rci > 0.0 { wi * (target / rci).sqrt() } else { *wi }
            })
            .collect();
        let sum: f64 = next.iter().sum();
        if sum > 0.0 {
            for x in next.iter_mut() {
                *x /= sum;
            }
        } else {
            next = vec![target; n];
        }
        next = project_capped(&next, cfg.max_weight);

        let delta = next.iter().zip(&w).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        w = next;
        if delta < cfg.tol {
            converged = true;
            break;
        }
    }
    (w, iterations, !converged)
}

/// Projected gradient descent on an arbitrary objective.
///
/// `grad` returns the ascent direction of the value being *maximized*; callers
/// minimizing something pass the negated gradient. Backtracking keeps the step
/// stable across the very different curvatures of variance and Sharpe.
fn projected_gradient<F, G>(
    n: usize,
    cfg: &OptimizerConfig,
    value: F,
    grad: G,
) -> (Vec<f64>, usize, bool)
where
    F: Fn(&[f64]) -> f64,
    G: Fn(&[f64]) -> Vec<f64>,
{
    let mut w = project_capped(&vec![1.0 / n as f64; n], cfg.max_weight);
    let mut step = 1.0;
    let mut iterations = 0;
    let mut converged = false;

    for it in 1..=cfg.max_iter {
        iterations = it;
        let current = value(&w);
        let g = grad(&w);

        // Backtracking: shrink the step until it does not make things worse.
        let mut candidate = w.clone();
        let mut accepted = false;
        for _ in 0..40 {
            let trial: Vec<f64> =
                w.iter().zip(&g).map(|(wi, gi)| wi + step * gi).collect();
            let trial = project_capped(&trial, cfg.max_weight);
            if value(&trial) >= current {
                candidate = trial;
                accepted = true;
                break;
            }
            step *= 0.5;
            if step < 1e-16 {
                break;
            }
        }
        if !accepted {
            converged = true; // no improving step exists at any scale
            break;
        }

        let delta = candidate.iter().zip(&w).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        w = candidate;
        step *= 1.1; // creep back up so later iterations are not stuck small
        if delta < cfg.tol {
            converged = true;
            break;
        }
    }
    (w, iterations, !converged)
}

/// Solve for weights.
///
/// `returns[i]` is asset `i`'s daily return series. Series must be **date
/// aligned**: index `k` is the same calendar date for every asset. See
/// [`covariance`] for what happens if they are not, and why this function cannot
/// fix it for you.
///
/// Degenerate inputs (no assets, fewer than 2 observations, an all-zero
/// covariance matrix) fall back to equal weight rather than failing — a
/// rebalance in a dynamic backtest must always produce a usable allocation.
pub fn optimize_weights(returns: &[Vec<f64>], cfg: &OptimizerConfig) -> WeightSolution {
    let n = returns.len();
    if n == 0 {
        return WeightSolution::degenerate(0);
    }
    if n == 1 {
        let mut sol = WeightSolution::degenerate(1);
        sol.expected_return = mean_returns(returns)[0];
        sol.expected_volatility = covariance(returns)[0][0].max(0.0).sqrt();
        return sol;
    }

    let raw_cov = covariance(returns);
    let cov = shrink_covariance(&raw_cov, cfg.shrinkage);
    let mu = mean_returns(returns);

    // An all-zero diagonal means no asset moved in the window; there is nothing
    // to optimize over.
    if cov.iter().enumerate().all(|(i, row)| row[i] <= 0.0) {
        return WeightSolution::degenerate(n);
    }

    // NOTE: a *single* zero-variance asset is a known hazard here, and it is
    // deliberately not handled at this layer. See
    // `a_single_flat_asset_is_treated_as_riskless` for why the obvious fix is
    // wrong: this function cannot distinguish "the window holds no information
    // about this asset" from "the strategy was in cash", and cash genuinely is
    // riskless. Excluding zero-variance columns breaks min-variance's defining
    // guarantee whenever a strategy sits out a window. The fix belongs upstream,
    // where first-bar dates are known.

    let (weights, iterations, hit_limit) = match cfg.objective {
        Objective::EqualWeight => {
            (project_capped(&vec![1.0 / n as f64; n], cfg.max_weight), 0, false)
        }
        Objective::InverseVolatility => {
            (project_capped(&inverse_volatility(&cov), cfg.max_weight), 0, false)
        }
        Objective::RiskParity => solve_risk_parity(&cov, cfg),
        Objective::MinVariance => {
            // Maximize −variance, i.e. ascend −2Σw.
            let c = cov.clone();
            let c2 = cov.clone();
            projected_gradient(
                n,
                cfg,
                move |w| -quad_form(&c, w),
                move |w| mat_vec(&c2, w).iter().map(|x| -2.0 * x).collect(),
            )
        }
        Objective::MaxSharpe => {
            let c = cov.clone();
            let c2 = cov.clone();
            let m = mu.clone();
            let m2 = mu.clone();
            projected_gradient(
                n,
                cfg,
                move |w| {
                    let sd = quad_form(&c, w).sqrt();
                    if sd > 0.0 { dot(&m, w) / sd } else { 0.0 }
                },
                move |w| {
                    // d/dw (μᵀw / σ) = μ/σ − (μᵀw)·Σw/σ³
                    let var = quad_form(&c2, w);
                    let sd = var.sqrt();
                    if sd <= 0.0 {
                        return vec![0.0; w.len()];
                    }
                    let mw = dot(&m2, w);
                    let sw = mat_vec(&c2, w);
                    m2.iter()
                        .zip(&sw)
                        .map(|(mi, si)| mi / sd - mw * si / (sd * var))
                        .collect()
                },
            )
        }
    };

    WeightSolution {
        expected_volatility: quad_form(&cov, &weights).sqrt(),
        expected_return: dot(&mu, &weights),
        risk_contribution: risk_contribution(&cov, &weights),
        weights,
        iterations,
        hit_iteration_limit: hit_limit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random returns with a target volatility, so tests
    /// are reproducible without pulling in an RNG dependency.
    fn series(n: usize, daily_vol: f64, drift: f64, seed: u64) -> Vec<f64> {
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (0..n)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                // Top 32 bits → uniform(-1, 1), scaled to the requested vol.
                let u = ((state >> 32) as f64 / u32::MAX as f64) * 2.0 - 1.0;
                drift + u * daily_vol * 1.732 // ×√3 so uniform has the target sd
            })
            .collect()
    }

    fn sums_to_one(w: &[f64]) -> bool {
        (w.iter().sum::<f64>() - 1.0).abs() < 1e-6
    }

    fn all_non_negative(w: &[f64]) -> bool {
        w.iter().all(|&x| x >= -1e-12)
    }

    // ─── Projection ──────────────────────────────────────────────────────────

    #[test]
    fn projection_returns_a_valid_portfolio() {
        let w = project_to_simplex(&[0.5, 0.3, 0.9]);
        assert!(sums_to_one(&w), "sum = {}", w.iter().sum::<f64>());
        assert!(all_non_negative(&w));
    }

    #[test]
    fn projection_clips_negatives_to_zero() {
        // A short is not representable by the engine, so it must project out.
        let w = project_to_simplex(&[1.5, -0.5]);
        assert!(sums_to_one(&w));
        assert_eq!(w[1], 0.0);
        assert!((w[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn projection_is_identity_on_an_already_valid_vector() {
        let v = [0.25, 0.25, 0.5];
        let w = project_to_simplex(&v);
        for (a, b) in w.iter().zip(&v) {
            assert!((a - b).abs() < 1e-9, "{a} vs {b}");
        }
    }

    #[test]
    fn cap_bounds_concentration_and_still_sums_to_one() {
        let w = project_capped(&[0.9, 0.05, 0.05], Some(0.5));
        assert!(sums_to_one(&w), "sum = {}", w.iter().sum::<f64>());
        assert!(w.iter().all(|&x| x <= 0.5 + 1e-9), "{w:?}");
    }

    #[test]
    fn an_infeasible_cap_degrades_to_equal_weight() {
        // 0.2 across 3 names cannot reach 1.0; the cap is raised to 1/n.
        let w = project_capped(&[0.9, 0.05, 0.05], Some(0.2));
        assert!(sums_to_one(&w));
        assert!(w.iter().all(|&x| (x - 1.0 / 3.0).abs() < 1e-9), "{w:?}");
    }

    // ─── Estimation ──────────────────────────────────────────────────────────

    #[test]
    fn covariance_is_symmetric_and_annualized() {
        let r = vec![series(300, 0.01, 0.0, 1), series(300, 0.02, 0.0, 2)];
        let cov = covariance(&r);
        assert!((cov[0][1] - cov[1][0]).abs() < 1e-15);
        // Annualized vol of the 1%/day series should be near 1%·√252 ≈ 15.9%.
        let vol0 = cov[0][0].sqrt();
        assert!(vol0 > 0.10 && vol0 < 0.22, "vol0 = {vol0}");
        // The 2%/day series must be the more volatile of the two.
        assert!(cov[1][1] > cov[0][0]);
    }

    #[test]
    fn shrinkage_damps_covariance_but_preserves_variance() {
        let cov = vec![vec![4.0, 2.0], vec![2.0, 9.0]];
        let s = shrink_covariance(&cov, 0.5);
        assert_eq!(s[0][0], 4.0, "variance must be untouched");
        assert_eq!(s[1][1], 9.0);
        assert!((s[0][1] - 1.0).abs() < 1e-12, "off-diagonal halved");
    }

    #[test]
    fn full_shrinkage_discards_correlation_entirely() {
        let cov = vec![vec![4.0, 2.0], vec![2.0, 9.0]];
        let s = shrink_covariance(&cov, 1.0);
        assert_eq!(s[0][1], 0.0);
        assert_eq!(s[1][0], 0.0);
    }

    // ─── Objectives ──────────────────────────────────────────────────────────

    #[test]
    fn equal_weight_is_one_over_n() {
        let r = vec![series(100, 0.01, 0.0, 1), series(100, 0.03, 0.0, 2), series(100, 0.02, 0.0, 3)];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::EqualWeight));
        assert!(sol.weights.iter().all(|&w| (w - 1.0 / 3.0).abs() < 1e-9), "{:?}", sol.weights);
    }

    #[test]
    fn inverse_volatility_underweights_the_riskier_asset() {
        let r = vec![series(500, 0.005, 0.0, 7), series(500, 0.02, 0.0, 8)];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::InverseVolatility));
        assert!(sums_to_one(&sol.weights));
        assert!(sol.weights[0] > sol.weights[1], "{:?}", sol.weights);
    }

    #[test]
    fn min_variance_matches_the_closed_form_for_uncorrelated_assets() {
        // With zero correlation the optimum is w_i ∝ 1/σ_i². Shrinkage is
        // disabled so the comparison is against the exact sample covariance.
        let r = vec![series(600, 0.01, 0.0, 11), series(600, 0.02, 0.0, 12)];
        let cov = covariance(&r);
        let cfg = OptimizerConfig { shrinkage: 1.0, ..OptimizerConfig::new(Objective::MinVariance) };
        let sol = optimize_weights(&r, &cfg);

        let (v0, v1) = (cov[0][0], cov[1][1]);
        let expected0 = (1.0 / v0) / (1.0 / v0 + 1.0 / v1);
        assert!(
            (sol.weights[0] - expected0).abs() < 5e-3,
            "got {:?}, expected w0 ≈ {expected0}",
            sol.weights
        );
    }

    #[test]
    fn min_variance_beats_equal_weight_on_variance() {
        // The defining property: nothing may have lower in-sample variance.
        let r = vec![series(400, 0.004, 0.0, 21), series(400, 0.03, 0.0, 22), series(400, 0.01, 0.0, 23)];
        let cov = shrink_covariance(&covariance(&r), 0.2);
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::MinVariance));
        let eq = vec![1.0 / 3.0; 3];
        assert!(
            quad_form(&cov, &sol.weights) <= quad_form(&cov, &eq) + 1e-12,
            "min-var {:?} was not better than equal weight",
            sol.weights
        );
    }

    #[test]
    fn min_variance_concentrates_in_the_calm_asset() {
        let r = vec![series(400, 0.002, 0.0, 31), series(400, 0.04, 0.0, 32)];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::MinVariance));
        assert!(sol.weights[0] > 0.8, "{:?}", sol.weights);
    }

    #[test]
    fn risk_parity_equalizes_risk_contributions() {
        let r = vec![series(500, 0.005, 0.0, 41), series(500, 0.02, 0.0, 42), series(500, 0.01, 0.0, 43)];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::RiskParity));
        assert!(sums_to_one(&sol.weights));
        for rc in &sol.risk_contribution {
            assert!((rc - 1.0 / 3.0).abs() < 0.02, "contributions = {:?}", sol.risk_contribution);
        }
    }

    #[test]
    fn risk_parity_reduces_to_inverse_vol_when_uncorrelated() {
        // A known identity worth pinning: with zero correlation, equal risk
        // contribution is exactly inverse-volatility weighting.
        let r = vec![series(600, 0.005, 0.0, 51), series(600, 0.015, 0.0, 52)];
        let cfg = OptimizerConfig { shrinkage: 1.0, ..OptimizerConfig::new(Objective::RiskParity) };
        let rp = optimize_weights(&r, &cfg);
        let iv = optimize_weights(
            &r,
            &OptimizerConfig { shrinkage: 1.0, ..OptimizerConfig::new(Objective::InverseVolatility) },
        );
        for (a, b) in rp.weights.iter().zip(&iv.weights) {
            assert!((a - b).abs() < 1e-3, "rp {:?} vs iv {:?}", rp.weights, iv.weights);
        }
    }

    #[test]
    fn risk_parity_holds_every_asset() {
        // Equalizing risk shares can never zero a name out — that is the point
        // of it versus min-variance.
        let r = vec![series(400, 0.002, 0.0, 61), series(400, 0.05, 0.0, 62)];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::RiskParity));
        assert!(sol.weights.iter().all(|&w| w > 0.01), "{:?}", sol.weights);
    }

    #[test]
    fn max_sharpe_prefers_the_better_paid_asset_at_equal_risk() {
        let r = vec![series(500, 0.01, 0.0008, 71), series(500, 0.01, 0.0, 72)];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::MaxSharpe));
        assert!(sol.weights[0] > sol.weights[1], "{:?}", sol.weights);
    }

    #[test]
    fn max_sharpe_beats_equal_weight_on_sharpe() {
        let r = vec![series(500, 0.008, 0.0006, 81), series(500, 0.02, 0.0002, 82), series(500, 0.012, 0.0004, 83)];
        let cfg = OptimizerConfig::new(Objective::MaxSharpe);
        let sol = optimize_weights(&r, &cfg);

        let cov = shrink_covariance(&covariance(&r), cfg.shrinkage);
        let mu = mean_returns(&r);
        let sharpe = |w: &[f64]| {
            let sd = quad_form(&cov, w).sqrt();
            if sd > 0.0 { dot(&mu, w) / sd } else { 0.0 }
        };
        let eq = vec![1.0 / 3.0; 3];
        assert!(sharpe(&sol.weights) >= sharpe(&eq) - 1e-9, "{:?}", sol.weights);
    }

    #[test]
    fn only_max_sharpe_declares_a_dependence_on_expected_returns() {
        assert!(Objective::MaxSharpe.uses_expected_returns());
        for o in [
            Objective::EqualWeight,
            Objective::InverseVolatility,
            Objective::MinVariance,
            Objective::RiskParity,
        ] {
            assert!(!o.uses_expected_returns(), "{o:?}");
        }
    }

    // ─── Constraints and invariants ──────────────────────────────────────────

    #[test]
    fn every_objective_returns_a_valid_long_only_portfolio() {
        let r = vec![series(300, 0.01, 0.0003, 91), series(300, 0.02, -0.0001, 92), series(300, 0.005, 0.0002, 93)];
        for objective in [
            Objective::EqualWeight,
            Objective::InverseVolatility,
            Objective::MinVariance,
            Objective::RiskParity,
            Objective::MaxSharpe,
        ] {
            let sol = optimize_weights(&r, &OptimizerConfig::new(objective));
            assert!(sums_to_one(&sol.weights), "{objective:?}: {:?}", sol.weights);
            assert!(all_non_negative(&sol.weights), "{objective:?}: {:?}", sol.weights);
        }
    }

    #[test]
    fn the_position_cap_is_respected_by_every_objective() {
        let r = vec![series(300, 0.002, 0.001, 101), series(300, 0.05, 0.0, 102), series(300, 0.03, 0.0, 103)];
        for objective in [
            Objective::EqualWeight,
            Objective::InverseVolatility,
            Objective::MinVariance,
            Objective::RiskParity,
            Objective::MaxSharpe,
        ] {
            let cfg = OptimizerConfig { max_weight: Some(0.5), ..OptimizerConfig::new(objective) };
            let sol = optimize_weights(&r, &cfg);
            assert!(
                sol.weights.iter().all(|&w| w <= 0.5 + 1e-6),
                "{objective:?} breached the cap: {:?}",
                sol.weights
            );
            assert!(sums_to_one(&sol.weights), "{objective:?}: {:?}", sol.weights);
        }
    }

    #[test]
    fn risk_contributions_always_sum_to_one() {
        let r = vec![series(300, 0.01, 0.0, 111), series(300, 0.02, 0.0, 112)];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::MinVariance));
        assert!(sums_to_one(&sol.risk_contribution), "{:?}", sol.risk_contribution);
    }

    // ─── Degenerate input ────────────────────────────────────────────────────

    #[test]
    fn no_assets_yields_no_weights() {
        let sol = optimize_weights(&[], &OptimizerConfig::default());
        assert!(sol.weights.is_empty());
    }

    #[test]
    fn a_single_asset_takes_the_whole_portfolio() {
        let sol = optimize_weights(&[series(100, 0.01, 0.0, 121)], &OptimizerConfig::default());
        assert_eq!(sol.weights.len(), 1);
        assert!((sol.weights[0] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_flat_window_falls_back_to_equal_weight() {
        // No asset moved, so there is no risk to allocate against. A dynamic
        // rebalance must still get a usable allocation rather than NaN.
        let r = vec![vec![0.0; 50], vec![0.0; 50]];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::MinVariance));
        assert!(sums_to_one(&sol.weights));
        assert!(sol.weights.iter().all(|&w| (w - 0.5).abs() < 1e-9), "{:?}", sol.weights);
    }

    #[test]
    fn a_single_flat_asset_is_treated_as_riskless() {
        // KNOWN LIMITATION, pinned deliberately rather than fixed here.
        //
        // B never moves inside the window, so min-variance loads up on it and
        // reports ~zero expected volatility for the book. When B is flat because
        // the window predates its first bar, that is wrong and dangerous: the
        // solver is allocating to the asset it has no information about.
        //
        // But this function cannot tell that case apart from a strategy that sat
        // in *cash* for the window, which produces an identically flat NAV series
        // and genuinely is riskless — and correctly so, since holding cash has no
        // variance. Excluding zero-variance columns therefore breaks
        // min-variance's defining guarantee (predicted vol never above equal
        // weight, which is always a feasible point) any time a strategy sits out
        // a window. That is not a hypothetical: it is what
        // `test_min_variance_beats_equal_weight_at_every_solve` catches.
        //
        // The distinction lives upstream, where per-asset first-bar dates are
        // known. Tracked in ROADMAP.md Horizon 5A.
        let r = vec![series(120, 0.012, 0.0003, 971), vec![0.0; 120]];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::MinVariance));
        assert!(sums_to_one(&sol.weights));
        assert!(
            sol.weights[1] > 0.99,
            "documenting current behaviour: min-variance loads the flat asset, got {:?}",
            sol.weights
        );
    }

    #[test]
    fn min_variance_never_predicts_more_risk_than_equal_weight() {
        // The defining guarantee: equal weight is always a feasible point, so the
        // minimiser can never do worse. Includes a flat asset, because that is
        // exactly the case where a zero-variance exclusion would violate it.
        let cases: Vec<Vec<Vec<f64>>> = vec![
            vec![series(200, 0.01, 0.0, 981), series(200, 0.02, 0.0, 982)],
            vec![series(200, 0.01, 0.0, 983), vec![0.0; 200]],
            vec![series(200, 0.005, 0.0, 984), series(200, 0.03, 0.0, 985), vec![0.0; 200]],
        ];
        for (i, r) in cases.iter().enumerate() {
            let mv = optimize_weights(r, &OptimizerConfig::new(Objective::MinVariance));
            let eq = optimize_weights(r, &OptimizerConfig::new(Objective::EqualWeight));
            assert!(
                mv.expected_volatility <= eq.expected_volatility + 1e-9,
                "case {i}: min-variance predicted {} vs equal weight {}",
                mv.expected_volatility,
                eq.expected_volatility
            );
        }
    }

    #[test]
    fn too_few_observations_falls_back_to_equal_weight() {
        let r = vec![vec![0.01], vec![0.02]];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::RiskParity));
        assert!(sums_to_one(&sol.weights));
        assert!(sol.weights.iter().all(|&w| w.is_finite()));
    }

    #[test]
    fn unequal_history_lengths_are_truncated_not_rejected() {
        let r = vec![series(300, 0.01, 0.0, 131), series(120, 0.02, 0.0, 132)];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::RiskParity));
        assert!(sums_to_one(&sol.weights));
        assert!(sol.weights.iter().all(|&w| w.is_finite()));
    }

    #[test]
    fn unequal_lengths_fall_back_to_the_most_recent_overlap() {
        // Unequal lengths mean the caller failed to align, and this function has
        // no dates to align with — but the guess it makes still matters. A's
        // history is wild for 200 bars then quiet for 100; B covers 100 bars of
        // moderate vol. Keeping the *trailing* overlap compares A's quiet stretch
        // against B and weights A up. Keeping the leading overlap — the previous
        // behaviour — would compare A's wild opening against B and weight it down,
        // i.e. answer a question about a period B was never measured over.
        let mut a = series(200, 0.05, 0.0, 991);
        a.extend(series(100, 0.001, 0.0, 992));
        let b = series(100, 0.01, 0.0, 993);

        let sol = optimize_weights(&[a, b], &OptimizerConfig::new(Objective::InverseVolatility));
        assert!(
            sol.weights[0] > 0.8,
            "expected the quiet trailing window to dominate, got {:?}",
            sol.weights
        );
    }

    #[test]
    fn identical_assets_are_split_evenly() {
        let s = series(400, 0.01, 0.0002, 141);
        let r = vec![s.clone(), s];
        let sol = optimize_weights(&r, &OptimizerConfig::new(Objective::RiskParity));
        assert!((sol.weights[0] - sol.weights[1]).abs() < 1e-3, "{:?}", sol.weights);
    }

    #[test]
    fn iterative_solvers_converge_well_inside_the_budget() {
        // Guards the cost of dynamic rebalancing: this runs once per rebalance
        // date, so silent max_iter exhaustion would be a real slowdown.
        let r = vec![series(500, 0.005, 0.0, 151), series(500, 0.02, 0.0, 152), series(500, 0.01, 0.0, 153)];
        for objective in [Objective::RiskParity, Objective::MinVariance, Objective::MaxSharpe] {
            let sol = optimize_weights(&r, &OptimizerConfig::new(objective));
            assert!(!sol.hit_iteration_limit, "{objective:?} hit the iteration limit");
        }
    }

    #[test]
    fn solutions_are_deterministic() {
        // Dynamic runs re-solve hundreds of times; a solver that wandered would
        // make results irreproducible.
        let r = vec![series(300, 0.01, 0.0003, 161), series(300, 0.02, 0.0, 162)];
        let cfg = OptimizerConfig::new(Objective::RiskParity);
        let a = optimize_weights(&r, &cfg);
        let b = optimize_weights(&r, &cfg);
        assert_eq!(a.weights, b.weights);
    }
}
