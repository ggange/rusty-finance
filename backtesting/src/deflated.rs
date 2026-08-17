//! Multiple-testing correction for parameter searches.
//!
//! The best cell of an `N`-cell parameter sweep is a biased estimator by
//! construction: search hard enough over pure noise and the maximum Sharpe grows
//! without any signal underneath it. [`crate::bootstrap`] cannot fix this — an
//! interval around a *selected* maximum has no valid frequentist reading, and a
//! grid of independent 95 % bands invites reading the selection as free.
//!
//! The Deflated Sharpe Ratio (Bailey & López de Prado, 2014) is the correction.
//! It asks how high the best of `N` Sharpe ratios would have got under the null
//! of no skill, given how much the Sharpes varied across the trials, and then
//! reports the probability that the observed winner beats *that* benchmark rather
//! than beating zero.
//!
//! # What it does not correct for
//!
//! `N` here is the size of one sweep. It cannot see the other strategies, the
//! other assets, or the earlier grids the researcher ran before this one, so a
//! DSR computed from a single sweep is an **upper bound on significance**. Use
//! [`DeflationConfig::trials_override`] to deflate against the honest count when
//! it is known. It also treats the trials as draws from one distribution, which
//! adjacent parameter values plainly violate — correlated trials mean the
//! effective `N` is smaller than the nominal one, in the optimistic direction.
//! Trial multiplicity across folds and assets needs the probability of backtest
//! overfitting, not this.

use serde::{Deserialize, Serialize};

use crate::stats::{
    kurtosis, normal_cdf, normal_quantile, population_std_dev, skewness, EULER_MASCHERONI,
    TRADING_DAYS,
};

/// One cell of the search, as the deflation sees it.
///
/// Carries the returns of *every* trial rather than just the winner's, so the
/// caller cannot pair one cell's Sharpe with another cell's return distribution
/// — a mismatch that would produce a plausible-looking number with no error.
pub struct Trial<'a> {
    /// Annualised Sharpe ratio, exactly as [`crate::metrics::Metrics`] reports it.
    pub annualized_sharpe: f64,
    /// The cell's periodic (per-bar) returns. Only the winner's are read, for
    /// skewness and kurtosis.
    pub returns: &'a [f64],
    /// Executed trades in this cell. Used only for reporting: a grid where most
    /// cells never traded has a Sharpe spread that is an artefact of degeneracy
    /// rather than a measure of the search space.
    pub trade_count: usize,
}

/// How hard to deflate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeflationConfig {
    /// When false, callers skip the correction entirely and report nothing.
    pub enabled: bool,
    /// Number of trials to deflate against. `None` uses the number of cells with
    /// a finite Sharpe, which is the most this sweep can see on its own.
    ///
    /// Raise it to account for searches outside this sweep — other strategy
    /// families, other assets, earlier grids. Values *below* the number of cells
    /// actually run are rejected rather than honoured: deflating against fewer
    /// trials than were demonstrably performed would weaken the correction on
    /// request, which is the one direction this module must not allow.
    pub trials_override: Option<usize>,
}

impl Default for DeflationConfig {
    fn default() -> Self {
        // On by default, and for the same reason the bootstrap is: the whole
        // point of principle 7 is that the correction is not opt-in.
        Self { enabled: true, trials_override: None }
    }
}

impl DeflationConfig {
    /// Correction disabled, for callers that only want the raw grid.
    pub fn off() -> Self {
        Self { enabled: false, trials_override: None }
    }
}

/// The deflation of one parameter search, reported alongside its grid.
///
/// Every Sharpe-dimensioned field is **annualised**, matching what the rest of
/// the platform displays. The arithmetic runs on periodic (per-bar) values
/// internally, because the probabilistic Sharpe ratio's `sqrt(T − 1)` factor is
/// only correct in the units the `T` observations are measured in.
#[derive(Debug, Serialize, Clone)]
pub struct SelectionCorrection {
    /// Always `"deflated_sharpe_ratio"`. Present so a stored response says which
    /// correction produced it once there is more than one.
    pub method: &'static str,
    /// `N`: the trial count actually deflated against.
    pub trials: usize,
    /// Cells in the grid with a finite Sharpe. Equals `trials` unless overridden.
    pub trials_run: usize,
    /// Cells that executed at least one trade.
    pub trials_that_traded: usize,
    /// Whether `trials` came from the caller rather than from the grid.
    pub trials_overridden: bool,
    /// Index of the winning cell in the grid, in the caller's own ordering.
    pub best_index: usize,
    /// Cells sharing the winning Sharpe exactly. Greater than one means the
    /// search did not really select this cell; the first was kept.
    pub tied_at_best: usize,
    /// `T`: return observations in the winning cell.
    pub observations: usize,
    /// The winner's annualised Sharpe — the number the "best combination" callout
    /// displays, repeated here so the correction is self-contained.
    pub sharpe_ratio: f64,
    /// `sqrt(Var[SR])` across trials, annualised. This is what sets the size of
    /// the correction: a flat search space deflates to nothing.
    pub trial_sharpe_std_dev: f64,
    /// `SR*`, annualised: the Sharpe the best of `N` trials would be expected to
    /// reach with no skill at all. Beating zero is not the bar; beating this is.
    pub expected_max_sharpe: f64,
    /// Skewness of the winner's returns. Negative lowers the deflated Sharpe:
    /// occasional large losses make a given mean/variance less trustworthy.
    pub skewness: f64,
    /// Kurtosis of the winner's returns, **non-excess** (3.0 for a normal
    /// sample). Fat tails lower the deflated Sharpe.
    pub kurtosis: f64,
    /// `PSR(0)`: the probability the true Sharpe exceeds zero. The uncorrected
    /// baseline, reported so the size of the deflation is visible rather than
    /// inferred.
    pub probabilistic_sharpe: f64,
    /// `PSR(SR*)`: the probability the true Sharpe exceeds what the search would
    /// have produced by luck. **The headline number.** Below ~0.95, the winning
    /// cell is not distinguishable from the best of `N` coin flips.
    pub deflated_sharpe: f64,
    /// Set when fewer than half the cells ever traded, so `trial_sharpe_std_dev`
    /// is measuring the gap between a strategy and a block of do-nothing cells
    /// rather than the shape of the search space.
    pub degenerate_trials_dominate: bool,
}

/// Deflate a parameter search's winning Sharpe ratio for the size of the search.
///
/// The winner is the arg-max of **Sharpe**, whatever metric the caller happens to
/// display. The deflation's distributional assumptions are Sharpe's; applying the
/// same formula to a CAGR arg-max would be wrong, and quietly wrong. Callers that
/// rank by another metric must say so where the result is shown.
///
/// # Errors
///
/// - fewer than two trials with a finite Sharpe — `SR*` needs `Φ⁻¹(1 − 1/N)`,
///   which is `+inf` at `N = 1`, and that is a real degeneracy rather than
///   something to clamp: a search of one is not a search
/// - a winning cell with fewer than two returns
/// - `trials_override` below the number of cells actually run
/// - a winner whose return distribution gives a non-positive variance for the
///   probabilistic Sharpe ratio
///
/// A failure means no correction is reported. It is never a reason for a sweep to
/// fail — the deflation is an enrichment, exactly as a bootstrap interval is.
pub fn deflate(
    trials: &[Trial<'_>],
    cfg: &DeflationConfig,
) -> Result<SelectionCorrection, String> {
    let finite: Vec<usize> = (0..trials.len())
        .filter(|&i| trials[i].annualized_sharpe.is_finite())
        .collect();
    let trials_run = finite.len();
    if trials_run < 2 {
        return Err(format!(
            "deflation needs at least 2 trials with a finite Sharpe, got {trials_run}"
        ));
    }

    let trials_overridden = cfg.trials_override.is_some();
    let n_trials = cfg.trials_override.unwrap_or(trials_run);
    if n_trials < trials_run {
        return Err(format!(
            "trials_override ({n_trials}) is below the {trials_run} trials actually run; \
             deflating against fewer trials than were performed would weaken the correction"
        ));
    }

    // Arg-max with an explicit tie count. Strict `>` keeps the first of a tie,
    // matching the walk-forward selector, and the count is reported rather than
    // hidden because an unbroken tie means the search did not select anything.
    let mut best_index = finite[0];
    for &i in &finite {
        if trials[i].annualized_sharpe > trials[best_index].annualized_sharpe {
            best_index = i;
        }
    }
    let best_sharpe = trials[best_index].annualized_sharpe;
    let tied_at_best = finite
        .iter()
        .filter(|&&i| trials[i].annualized_sharpe == best_sharpe)
        .count();

    let winner = &trials[best_index];
    let observations = winner.returns.len();
    if observations < 2 {
        return Err(format!(
            "the winning trial has {observations} returns; at least 2 are needed"
        ));
    }

    // Spread of Sharpe across trials, population variance. Cells that never
    // traded score exactly 0.0 and are counted anyway: their inclusion inflates
    // this spread and therefore `SR*`, which *lowers* the deflated Sharpe. That
    // is the conservative direction, and per guiding principle 7 the weaker
    // result is the one that ships. `degenerate_trials_dominate` lets a reader
    // see when the spread is an artefact.
    // Uses the shared estimator so an all-identical grid reports exactly zero
    // dispersion rather than the 1e-14 of floating-point dust a naive variance
    // leaves behind — which would otherwise make `DSR == PSR` untestable and
    // report a phantom correction on a search with nothing to correct.
    let sharpes: Vec<f64> = finite.iter().map(|&i| trials[i].annualized_sharpe).collect();
    let trial_sharpe_std_dev = population_std_dev(&sharpes);

    // Expected maximum of `N` draws under the null of no skill (Bailey & López de
    // Prado 2014, eqs. 10-11). The bracket grows like sqrt(2·ln N): each extra
    // order of magnitude of searching buys a higher in-sample maximum for free.
    let bracket = (1.0 - EULER_MASCHERONI)
        * normal_quantile(1.0 - 1.0 / n_trials as f64)
        + EULER_MASCHERONI
            * normal_quantile(1.0 - 1.0 / (n_trials as f64 * std::f64::consts::E));
    let expected_max_sharpe = trial_sharpe_std_dev * bracket;

    // Periodic units from here down: the `sqrt(T - 1)` factor below counts
    // observations, so the Sharpe it multiplies has to be per-observation.
    // `Metrics` annualises by multiplying by sqrt(252), so dividing inverts it
    // exactly — and the zero-variance sentinel of 0.0 is still 0.0 periodic.
    let root = TRADING_DAYS.sqrt();
    let sr_hat = best_sharpe / root;
    let sr_star = expected_max_sharpe / root;

    let skew = skewness(winner.returns);
    let kurt = kurtosis(winner.returns);

    // Variance of the Sharpe estimator under non-normal returns (Mertens 2002,
    // as used by the probabilistic Sharpe ratio). Negative skew and fat tails
    // both enlarge it, which shrinks the t-statistic below.
    let denominator = 1.0 - skew * sr_hat + ((kurt - 1.0) / 4.0) * sr_hat * sr_hat;
    if !(denominator > 0.0) || !denominator.is_finite() {
        return Err(format!(
            "the winning trial's return distribution gives a non-positive Sharpe variance \
             ({denominator}); skew {skew}, kurtosis {kurt}"
        ));
    }
    let scale = ((observations - 1) as f64).sqrt() / denominator.sqrt();

    let probabilistic_sharpe = normal_cdf(sr_hat * scale);
    let deflated_sharpe = normal_cdf((sr_hat - sr_star) * scale);

    let trials_that_traded = finite.iter().filter(|&&i| trials[i].trade_count > 0).count();

    Ok(SelectionCorrection {
        method: "deflated_sharpe_ratio",
        trials: n_trials,
        trials_run,
        trials_that_traded,
        trials_overridden,
        best_index,
        tied_at_best,
        observations,
        sharpe_ratio: best_sharpe,
        trial_sharpe_std_dev,
        expected_max_sharpe,
        skewness: skew,
        kurtosis: kurt,
        probabilistic_sharpe,
        deflated_sharpe,
        degenerate_trials_dominate: trials_that_traded * 2 < trials_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns with a chosen periodic mean and standard deviation, alternating so
    /// the series is symmetric and its skew is zero — every test below that is
    /// not *about* higher moments wants them out of the way.
    fn returns_with(mean: f64, std_dev: f64, n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| if i % 2 == 0 { mean + std_dev } else { mean - std_dev })
            .collect()
    }

    /// A grid whose Sharpes are `sharpes`, all cells sharing one return series.
    fn grid<'a>(sharpes: &[f64], returns: &'a [f64]) -> Vec<Trial<'a>> {
        sharpes
            .iter()
            .map(|&annualized_sharpe| Trial { annualized_sharpe, returns, trade_count: 10 })
            .collect()
    }

    #[test]
    fn the_expected_maximum_sharpe_matches_the_hand_computed_bailey_formula() {
        // 100 trials with an annualised Sharpe standard deviation of exactly 1.0.
        // Φ⁻¹(1 − 1/100)      = Φ⁻¹(0.99)          = 2.3263478740408408
        // Φ⁻¹(1 − 1/(100·e))  = Φ⁻¹(0.9963212056)  = 2.680210444966888
        // SR* = (1 − γ)·2.3263478740 + γ·2.6802104450
        //     = 0.4227843351·2.3263478740 + 0.5772156649·2.6802104450
        //     = 0.98354... + 1.54706... = 2.530602893201685
        //
        // The two quantiles come from Python's `statistics.NormalDist.inv_cdf`
        // (Wichura's AS241), so this is a cross-check against an independent
        // implementation of Φ⁻¹ rather than against our own Acklam-plus-Halley
        // one. Agreement to the last bit is what the tolerance below allows for.
        //
        // Trials at ±1.0 around zero have population standard deviation 1.0, and
        // the winner at +1.0 does not enter this quantity at all.
        let returns = returns_with(0.0005, 0.01, 500);
        let sharpes: Vec<f64> = (0..100).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let out = deflate(&grid(&sharpes, &returns), &DeflationConfig::default()).unwrap();

        assert!((out.trial_sharpe_std_dev - 1.0).abs() < 1e-12);
        assert!(
            (out.expected_max_sharpe - 2.530_602_893_201_685).abs() < 1e-12,
            "SR* came out {}, AS241 gives 2.530602893201685",
            out.expected_max_sharpe,
        );
    }

    #[test]
    fn more_trials_at_the_same_winning_sharpe_lower_the_deflated_sharpe() {
        // The property the whole module exists for. The winner is held fixed at
        // an annualised Sharpe of 2.0 and the spread of the search is held fixed
        // too; only the number of cells grows. A correction that ignored the
        // search size would return the same number three times.
        let returns = returns_with(0.0008, 0.01, 1000);
        let mut previous = f64::INFINITY;
        for &n in &[4_usize, 40, 400] {
            let mut sharpes = vec![2.0];
            // Fill the rest symmetrically about zero so Var[SR] barely moves.
            sharpes.extend((1..n).map(|i| if i % 2 == 0 { 0.5 } else { -0.5 }));
            let out = deflate(&grid(&sharpes, &returns), &DeflationConfig::default()).unwrap();
            assert!(
                out.deflated_sharpe < previous,
                "{n} trials gave DSR {} which is not below the previous {previous}",
                out.deflated_sharpe,
            );
            previous = out.deflated_sharpe;
        }
    }

    #[test]
    fn identical_trials_deflate_to_the_probabilistic_sharpe() {
        // The contrastive partner of the test above: 400 trials, but no spread
        // across them, so there is nothing for the maximum to have been drawn
        // from and the correction vanishes. Proves the deflation is driven by
        // trial *dispersion*, not by the trial count alone — which is what
        // distinguishes it from a plain Bonferroni-style penalty.
        let returns = returns_with(0.0008, 0.01, 1000);
        let out = deflate(&grid(&vec![1.4; 400], &returns), &DeflationConfig::default()).unwrap();

        assert_eq!(out.trial_sharpe_std_dev, 0.0);
        assert_eq!(out.expected_max_sharpe, 0.0);
        assert_eq!(out.deflated_sharpe, out.probabilistic_sharpe);
        assert_eq!(out.tied_at_best, 400, "all 400 cells tie, and that is reported");
    }

    #[test]
    fn the_deflated_sharpe_never_exceeds_the_probabilistic_sharpe() {
        // SR* is non-negative for any N ≥ 2, so the correction can only ever move
        // the verdict in one direction.
        let returns = returns_with(0.001, 0.012, 600);
        for n in [2_usize, 3, 17, 250] {
            let sharpes: Vec<f64> =
                (0..n).map(|i| 1.5 - 0.1 * (i % 7) as f64).collect();
            let out = deflate(&grid(&sharpes, &returns), &DeflationConfig::default()).unwrap();
            assert!(out.expected_max_sharpe >= 0.0, "SR* went negative at n={n}");
            assert!(
                out.deflated_sharpe <= out.probabilistic_sharpe,
                "at n={n} DSR {} exceeded PSR {}",
                out.deflated_sharpe,
                out.probabilistic_sharpe,
            );
        }
    }

    #[test]
    fn negative_skew_lowers_the_deflated_sharpe() {
        // A series of many small losses punctuated by large gains, against its own
        // reflection about the mean. Mirroring flips the sign of the third moment
        // and leaves the mean, the variance and the *fourth* moment untouched, so
        // skewness is the only input that differs — which is what makes this a
        // test of skewness rather than of "I perturbed the series and DSR moved".
        // Pinned separately from kurtosis because the sign convention on each is
        // easy to invert.
        let mut right_tailed = vec![-0.002_f64; 800];
        for i in (0..800).step_by(40) { right_tailed[i] = 0.05; }
        let mean = right_tailed.iter().sum::<f64>() / 800.0;
        let left_tailed: Vec<f64> = right_tailed.iter().map(|r| 2.0 * mean - r).collect();

        let cfg = DeflationConfig::default();
        let sharpes = vec![1.5, 0.9, 0.3, -0.2, 0.6];
        let right = deflate(&grid(&sharpes, &right_tailed), &cfg).unwrap();
        let left = deflate(&grid(&sharpes, &left_tailed), &cfg).unwrap();

        assert!(right.skewness > 1.0, "expected a strongly right-tailed base, got {}", right.skewness);
        assert!(
            (left.skewness + right.skewness).abs() < 1e-12,
            "the mirror should have exactly the opposite skew: {} vs {}",
            left.skewness,
            right.skewness,
        );
        assert!(
            (left.kurtosis - right.kurtosis).abs() < 1e-12,
            "the mirror must leave kurtosis alone, or this test is not about skew",
        );
        assert!(
            left.deflated_sharpe < right.deflated_sharpe,
            "negative skew raised DSR from {} to {}",
            right.deflated_sharpe,
            left.deflated_sharpe,
        );
    }

    #[test]
    fn fat_tails_lower_the_deflated_sharpe() {
        let thin = returns_with(0.001, 0.01, 800);
        // One outlier pushed further above the mean and one pushed the same
        // distance below it. The base series alternates about its mean, so an even
        // index sits above and an odd index below — perturbing one of each in
        // opposite directions leaves the two deviations exactly antisymmetric, so
        // the third moment still cancels to zero while the fourth grows. Choosing
        // two same-parity indices instead would introduce positive skew and the
        // test would measure that instead, in the opposite direction.
        let mut fat = thin.clone();
        fat[200] += 0.09;
        fat[601] -= 0.09;

        let cfg = DeflationConfig::default();
        let sharpes = vec![1.5, 0.9, 0.3, -0.2, 0.6];
        let base = deflate(&grid(&sharpes, &thin), &cfg).unwrap();
        let heavy = deflate(&grid(&sharpes, &fat), &cfg).unwrap();

        assert!(base.skewness.abs() < 1e-12 && heavy.skewness.abs() < 1e-12,
            "both series must stay symmetric, or this test is not about kurtosis: {} vs {}",
            base.skewness, heavy.skewness);
        assert!(heavy.kurtosis > base.kurtosis, "the perturbed series should be fatter-tailed");
        assert!(
            heavy.deflated_sharpe < base.deflated_sharpe,
            "fat tails raised DSR from {} to {}",
            base.deflated_sharpe,
            heavy.deflated_sharpe,
        );
    }

    #[test]
    fn a_longer_sample_raises_the_deflated_sharpe_at_the_same_sharpe() {
        // The sqrt(T − 1) factor: the same effect size measured over more bars is
        // more believable. This is the one input a researcher cannot fake by
        // searching harder.
        let cfg = DeflationConfig::default();
        let sharpes = vec![1.8, 1.0, 0.4, -0.1, 0.7, 0.2];
        let short = returns_with(0.0009, 0.01, 120);
        let long = returns_with(0.0009, 0.01, 2000);

        let a = deflate(&grid(&sharpes, &short), &cfg).unwrap();
        let b = deflate(&grid(&sharpes, &long), &cfg).unwrap();
        assert_eq!(a.observations, 120);
        assert_eq!(b.observations, 2000);
        assert!(
            b.deflated_sharpe > a.deflated_sharpe,
            "2000 bars gave DSR {} against {} on 120 bars",
            b.deflated_sharpe,
            a.deflated_sharpe,
        );
    }

    #[test]
    fn a_single_trial_is_rejected_rather_than_deflated_against_infinity() {
        // Φ⁻¹(1 − 1/1) is +inf. A search of one is not a search, and reporting
        // some clamped finite penalty would invent a correction that does not
        // exist.
        let returns = returns_with(0.001, 0.01, 300);
        let err = deflate(&grid(&[1.2], &returns), &DeflationConfig::default()).unwrap_err();
        assert!(err.contains("at least 2 trials"), "unexpected message: {err}");
        assert!(deflate(&[], &DeflationConfig::default()).is_err());
    }

    #[test]
    fn a_non_finite_trial_is_dropped_rather_than_poisoning_the_variance() {
        let returns = returns_with(0.001, 0.01, 300);
        let out = deflate(
            &grid(&[1.2, f64::NAN, 0.4, f64::INFINITY], &returns),
            &DeflationConfig::default(),
        )
        .unwrap();
        assert_eq!(out.trials_run, 2, "only the two finite cells count as trials");
        assert!(out.trial_sharpe_std_dev.is_finite());
        assert_eq!(out.best_index, 0, "the index is into the caller's grid, not the filtered set");
    }

    #[test]
    fn a_winning_trial_too_short_to_measure_is_rejected() {
        let returns = vec![0.01];
        let err = deflate(&grid(&[1.2, 0.4], &returns), &DeflationConfig::default()).unwrap_err();
        assert!(err.contains("at least 2 are needed"), "unexpected message: {err}");
    }

    #[test]
    fn a_grid_where_nothing_traded_reports_a_deflated_sharpe_of_one_half() {
        // Every cell flat: Sharpe 0 everywhere, so there is no dispersion, no
        // correction, and a coin flip is the honest answer. The point is that it
        // returns a number rather than a NaN.
        let flat = vec![0.0; 300];
        let trials: Vec<Trial> = (0..20)
            .map(|_| Trial { annualized_sharpe: 0.0, returns: &flat, trade_count: 0 })
            .collect();
        let out = deflate(&trials, &DeflationConfig::default()).unwrap();

        assert_eq!(out.deflated_sharpe, 0.5);
        assert_eq!(out.trials_that_traded, 0);
        assert!(out.degenerate_trials_dominate, "a grid that never traded must say so");
    }

    #[test]
    fn a_mostly_idle_grid_is_flagged_but_still_deflated() {
        let returns = returns_with(0.001, 0.01, 500);
        let flat = vec![0.0; 500];
        let mut trials = vec![Trial { annualized_sharpe: 1.6, returns: &returns, trade_count: 8 }];
        trials.extend((0..9).map(|_| Trial {
            annualized_sharpe: 0.0,
            returns: &flat,
            trade_count: 0,
        }));
        let out = deflate(&trials, &DeflationConfig::default()).unwrap();

        assert_eq!(out.trials_that_traded, 1);
        assert!(out.degenerate_trials_dominate);
        // Still a real number: the flag informs the reading, it does not suppress
        // the correction.
        assert!(out.deflated_sharpe > 0.0 && out.deflated_sharpe < 1.0);
    }

    #[test]
    fn an_override_deflates_against_the_larger_trial_count() {
        // The honest N includes searches this sweep cannot see. Supplying it must
        // make the verdict stricter, never looser.
        let returns = returns_with(0.0009, 0.01, 1000);
        let sharpes = vec![2.0, 1.1, 0.5, -0.3, 0.8, 0.1, 1.4, -0.6];
        let plain = deflate(&grid(&sharpes, &returns), &DeflationConfig::default()).unwrap();
        let wide = deflate(
            &grid(&sharpes, &returns),
            &DeflationConfig { enabled: true, trials_override: Some(5_000) },
        )
        .unwrap();

        assert_eq!(plain.trials, 8);
        assert!(!plain.trials_overridden);
        assert_eq!(wide.trials, 5_000);
        assert!(wide.trials_overridden);
        assert!(wide.expected_max_sharpe > plain.expected_max_sharpe);
        assert!(
            wide.deflated_sharpe < plain.deflated_sharpe,
            "deflating against 5000 trials gave {} against {} for 8",
            wide.deflated_sharpe,
            plain.deflated_sharpe,
        );
    }

    #[test]
    fn an_override_below_the_grid_size_is_rejected() {
        let returns = returns_with(0.001, 0.01, 400);
        let err = deflate(
            &grid(&[1.5, 0.9, 0.2, -0.1], &returns),
            &DeflationConfig { enabled: true, trials_override: Some(2) },
        )
        .unwrap_err();
        assert!(err.contains("below the 4 trials actually run"), "unexpected message: {err}");
    }

    #[test]
    fn the_grid_arg_max_is_reported_with_its_tie_count() {
        let returns = returns_with(0.001, 0.01, 400);
        let out = deflate(&grid(&[0.4, 1.7, 0.9, 1.7], &returns), &DeflationConfig::default())
            .unwrap();
        assert_eq!(out.best_index, 1, "the first of a tie is kept");
        assert_eq!(out.tied_at_best, 2);
        assert_eq!(out.sharpe_ratio, 1.7);
    }

    #[test]
    fn a_disabled_config_is_the_callers_business_not_the_functions() {
        // `deflate` has no opinion on `enabled`; it is the caller that skips the
        // call. Pinned so the flag does not grow a second meaning here.
        let returns = returns_with(0.001, 0.01, 400);
        let cfg = DeflationConfig::off();
        assert!(!cfg.enabled);
        assert!(deflate(&grid(&[1.5, 0.4], &returns), &cfg).is_ok());
    }
}
