//! Numeric primitives shared across the analytics modules.
//!
//! Small on purpose. Something belongs here only when a second module needs it
//! and a second copy would be a place for the two to drift apart — the quantile
//! estimator below is exactly that case, having already shipped once as a
//! sign-inverting bug.

/// Trading days per year, the annualisation factor used everywhere in the crate.
///
/// Assumes daily bars. Weekly or monthly candles are annualised as though they
/// were daily and come out wrong by the frequency ratio; see
/// [`crate::metrics::Metrics::cagr`].
pub(crate) const TRADING_DAYS: f64 = 252.0;

/// Euler–Mascheroni constant, γ. Appears in the expected maximum of `N` draws
/// from a normal distribution, which is what
/// [`crate::deflated`] deflates a sweep's winning Sharpe against.
pub(crate) const EULER_MASCHERONI: f64 = 0.577_215_664_901_532_9;

/// Standard normal CDF, Φ(z), via Hart's (1968) rational approximation in the
/// formulation given by West (2005).
///
/// Relative error is around 1e-15 across the whole real line, including the far
/// tails. That precision is the reason for choosing this over the more commonly
/// copied Abramowitz & Stegun 7.1.26 series: A&S is accurate to 1.5e-7
/// *absolute*, which caps a probability near one at four usable decimals — and
/// "the deflated Sharpe is 0.9997" is exactly the reading that has to be
/// trustworthy, since it is the difference between a finding and a coin flip.
///
/// Saturates to 0.0 / 1.0 beyond |z| = 37, where the tail mass is below the
/// smallest normal `f64`.
pub(crate) fn normal_cdf(z: f64) -> f64 {
    let abs = z.abs();
    let tail = if abs > 37.0 {
        0.0
    } else {
        let decay = (-abs * abs / 2.0).exp();
        if abs < std::f64::consts::SQRT_2 * 5.0 {
            // Rational approximation, accurate through the body of the
            // distribution. At z = 0 the numerator is exactly half the
            // denominator, so Φ(0) comes out as exactly 0.5.
            let mut num = 3.526_249_659_989_11E-2 * abs + 0.700_383_064_443_688;
            num = num * abs + 6.373_962_203_531_65;
            num = num * abs + 33.912_866_078_383;
            num = num * abs + 112.079_291_497_871;
            num = num * abs + 221.213_596_169_931;
            num = num * abs + 220.206_867_912_376;
            let mut den = 8.838_834_764_831_84E-2 * abs + 1.755_667_163_182_64;
            den = den * abs + 16.064_177_579_207;
            den = den * abs + 86.780_732_202_946_1;
            den = den * abs + 296.564_248_779_674;
            den = den * abs + 637.333_633_378_831;
            den = den * abs + 793.826_512_519_948;
            den = den * abs + 440.413_735_824_752;
            decay * num / den
        } else {
            // Continued-fraction expansion of the Mills ratio, which keeps its
            // *relative* accuracy where the rational form would lose it to
            // cancellation.
            let mut frac = abs + 0.65;
            frac = abs + 4.0 / frac;
            frac = abs + 3.0 / frac;
            frac = abs + 2.0 / frac;
            frac = abs + 1.0 / frac;
            decay / (frac * 2.506_628_274_631)
        }
    };
    if z > 0.0 { 1.0 - tail } else { tail }
}

/// Standard normal quantile, Φ⁻¹(p), via Acklam's rational approximation
/// (relative error 1.15e-9) followed by one Halley step against [`normal_cdf`],
/// which takes the result to roughly machine precision.
///
/// Returns `-inf` at `p == 0.0` and `+inf` at `p == 1.0`, and `NaN` outside
/// `[0, 1]`. Callers must guard: the expected-maximum formula in
/// [`crate::deflated`] evaluates Φ⁻¹(1 − 1/N), which is `-inf`… `+inf` at
/// `N == 1`, and that is a real degeneracy rather than something to clamp away.
pub(crate) fn normal_quantile(p: f64) -> f64 {
    if p.is_nan() || p < 0.0 || p > 1.0 { return f64::NAN; }
    if p == 0.0 { return f64::NEG_INFINITY; }
    if p == 1.0 { return f64::INFINITY; }

    const A: [f64; 6] = [
        -3.969_683_028_665_376E1, 2.209_460_984_245_205E2, -2.759_285_104_469_687E2,
        1.383_577_518_672_690E2, -3.066_479_806_614_716E1, 2.506_628_277_459_239E0,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406E1, 1.615_858_368_580_409E2, -1.556_989_798_598_866E2,
        6.680_131_188_771_972E1, -1.328_068_155_288_572E1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293E-3, -3.223_964_580_411_365E-1, -2.400_758_277_161_838E0,
        -2.549_732_539_343_734E0, 4.374_664_141_464_968E0, 2.938_163_982_698_783E0,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462E-3, 3.224_671_290_700_398E-1,
        2.445_134_137_142_996E0, 3.754_408_661_907_416E0,
    ];
    const P_LOW: f64 = 0.024_25;

    let tail = |q: f64| -> f64 {
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };

    let mut z = if p < P_LOW {
        tail((-2.0 * p.ln()).sqrt())
    } else if p <= 1.0 - P_LOW {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        -tail((-2.0 * (1.0 - p).ln()).sqrt())
    };

    // Halley refinement. Skipped in the extreme tails, where `exp(z²/2)`
    // overflows and the unrefined approximation is already the better answer.
    if z.abs() < 30.0 {
        let err = normal_cdf(z) - p;
        let step = err * (2.0 * std::f64::consts::PI).sqrt() * (z * z / 2.0).exp();
        let refined = z - step / (1.0 + z * step / 2.0);
        if refined.is_finite() { z = refined; }
    }
    z
}

/// Population (biased) skewness: `mean((x − μ)³) / σ³`.
///
/// Population rather than sample moments to match [`crate::metrics::Metrics`],
/// which divides by `n` when computing the standard deviation behind Sharpe. The
/// two are read together, so they must share a convention.
///
/// Returns `0.0` for a series with fewer than two points or zero variance — a
/// constant series has no shape, and zero is the value that leaves the
/// probabilistic-Sharpe denominator behaving as it would for a normal sample.
pub(crate) fn skewness(values: &[f64]) -> f64 {
    let (mean, std_dev) = match moments(values) {
        Some(m) => m,
        None => return 0.0,
    };
    let n = values.len() as f64;
    values.iter().map(|v| ((v - mean) / std_dev).powi(3)).sum::<f64>() / n
}

/// Population (biased) kurtosis: `mean((x − μ)⁴) / σ⁴`.
///
/// **Non-excess.** A normal sample gives 3.0, not 0.0. Stated explicitly because
/// "kurtosis" means the excess form about half the time in finance code, and the
/// probabilistic Sharpe ratio takes the non-excess one — substituting the other
/// shifts the result silently rather than failing.
///
/// Returns `3.0` for a series with fewer than two points or zero variance, for
/// the same reason [`skewness`] returns zero: the neutral, normal-sample value.
pub(crate) fn kurtosis(values: &[f64]) -> f64 {
    let (mean, std_dev) = match moments(values) {
        Some(m) => m,
        None => return 3.0,
    };
    let n = values.len() as f64;
    values.iter().map(|v| ((v - mean) / std_dev).powi(4)).sum::<f64>() / n
}

/// Population standard deviation, snapped to exactly `0.0` when the spread is
/// only floating-point dust.
///
/// Degeneracy is judged *relative to the scale of the data*, not against exact
/// zero, and that distinction is load-bearing. Ten copies of `0.02` do not sum to
/// `0.2` in binary floating point, so each deviation from the mean is a few times
/// 1e-18 rather than zero. Two consequences follow, in opposite directions:
/// dividing that dust by itself turns rounding noise into a skewness of 1.0,
/// while leaving it as a nonzero standard deviation makes "these values are all
/// the same" untestable with `== 0.0`. Standardised moments amplify dust by
/// construction, which is exactly why the guard cannot be an equality.
///
/// The 1e-12 factor sits several orders of magnitude above that dust and many
/// below any return series with a volatility worth measuring.
///
/// Returns `0.0` for a series shorter than two values.
pub(crate) fn population_std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 { return 0.0; }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();
    if !std_dev.is_finite() { return std_dev; }
    let scale = values.iter().fold(mean.abs(), |m, v| m.max(v.abs()));
    if std_dev <= 1e-12 * scale { 0.0 } else { std_dev }
}

/// Mean and population standard deviation, or `None` when the series is too
/// short or degenerate for a standardised moment to mean anything.
fn moments(values: &[f64]) -> Option<(f64, f64)> {
    if values.len() < 2 { return None; }
    let std_dev = population_std_dev(values);
    if std_dev == 0.0 || !std_dev.is_finite() { return None; }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    Some((mean, std_dev))
}

/// Sort in place, ascending. `f64` is only `PartialOrd`, so `sort()` is
/// unavailable.
///
/// **NaN yields an unspecified order, not a sorted one.** Treating NaN as equal
/// to everything makes the comparator intransitive, so a NaN anywhere in the
/// slice can leave finite values out of order around it — it does not merely
/// misplace the NaN itself. The result stays deterministic and never panics,
/// which is all any caller here needs, because a NaN in a return series means
/// the equity curve is already broken upstream. Callers that must be correct
/// under NaN have to filter first.
///
/// This is the crate's one float sort, so at least the same input cannot order
/// differently in one module than another.
pub(crate) fn sort_ascending(values: &mut [f64]) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
}

/// Index of the `q`-quantile in an ascending series of `n` observations, using
/// the lower estimator `ceil(q·n) − 1`: the tail is the worst `ceil(q·n)`
/// observations, and the quantile is the mildest of them.
///
/// Using `floor(q·n)` instead selects one order statistic too far into the body
/// of the distribution, which always understates the tail and can invert the
/// sign outright — with exactly 5 losing days in 100, `floor` lands on the first
/// *winning* day and reports a positive VaR-95.
///
/// Used for both historical VaR/CVaR ([`crate::risk`]) and bootstrap percentile
/// endpoints ([`crate::bootstrap`]), so the two agree on what a 5 % tail means.
///
/// `q · n` is snapped to a whole number when it lands within float dust of one,
/// because `q` is often *computed* rather than literal and binary floating point
/// does not represent decimal tail probabilities exactly. `(1.0 − 0.95) / 2.0`
/// is `0.025000000000000022`, so a bare `ceil` of `q · 1000` gives 26 rather than
/// 25 and silently takes one extra observation into the tail.
pub(crate) fn tail_index(q: f64, n: usize) -> usize {
    debug_assert!(n > 0, "tail_index requires a non-empty series");
    let exact = q * n as f64;
    let whole = if (exact - exact.round()).abs() < 1e-9 {
        exact.round()
    } else {
        exact.ceil()
    };
    let k = whole.max(1.0) as usize;
    k.min(n) - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_percent_tail_of_one_hundred_is_the_fifth_observation() {
        // 5 % of 100 is exactly 5 observations, so the quantile is the last of
        // them: index 4, zero-based. floor() would give index 5.
        assert_eq!(tail_index(0.05, 100), 4);
    }

    #[test]
    fn a_tail_narrower_than_one_observation_still_selects_the_worst() {
        // 1 % of 20 is 0.2 observations. There is no fifth of a day, so the tail
        // rounds up to the single worst one rather than collapsing to nothing.
        assert_eq!(tail_index(0.01, 20), 0);
    }

    #[test]
    fn the_tail_index_never_runs_off_the_end() {
        assert_eq!(tail_index(1.0, 10), 9);
        assert_eq!(tail_index(2.0, 10), 9);
    }

    #[test]
    fn a_computed_tail_probability_is_not_widened_by_float_dust() {
        // (1.0 - 0.95) / 2.0 is 0.025000000000000022, not 0.025. Without the
        // snap, ceil(q·1000) is 26 and the tail quietly gains an observation.
        let q = (1.0 - 0.95) / 2.0;
        assert_eq!(tail_index(q, 1000), 24);
        // And the honest non-integer case still rounds up, as VaR needs.
        assert_eq!(tail_index(0.026, 1000), 25);
    }

    #[test]
    fn the_normal_cdf_matches_published_values() {
        // Φ(0) is exactly a half, and the rational form delivers it exactly
        // because its numerator at zero is exactly half its denominator.
        assert_eq!(normal_cdf(0.0), 0.5);
        // The two-sided 95 % critical value: Φ(1.959963985) = 0.975.
        assert!((normal_cdf(1.959_963_984_540_054) - 0.975).abs() < 1e-14);
        // Φ(-1) = 0.15865525393145705, Φ(2) = 0.9772498680518208.
        assert!((normal_cdf(-1.0) - 0.158_655_253_931_457_05).abs() < 1e-14);
        assert!((normal_cdf(2.0) - 0.977_249_868_051_820_8).abs() < 1e-14);
        // Symmetry, since the deflation reads both tails.
        assert!((normal_cdf(1.3) + normal_cdf(-1.3) - 1.0).abs() < 1e-15);
    }

    #[test]
    fn the_normal_cdf_stays_accurate_in_the_far_tail() {
        // Φ(-8) = 6.220960574271786e-16. This is the property Abramowitz &
        // Stegun 7.1.26 cannot deliver: its 1.5e-7 absolute error would return
        // something indistinguishable from zero here, and a *relative* check is
        // what a probability near one needs.
        //
        // The tail branch is a truncated continued fraction rather than the
        // rational form used through the body, so it holds ~1e-9 relative here
        // rather than the ~1e-15 available near the centre. Measured 8.7e-9, so
        // the tolerance below reads as drift from a known number.
        let phi = normal_cdf(-8.0);
        assert!(
            (phi / 6.220_960_574_271_786E-16 - 1.0).abs() < 1e-7,
            "Φ(-8) came out {phi:e}, expected 6.220960574e-16",
        );
        // Beyond the representable tail it saturates rather than returning junk.
        assert_eq!(normal_cdf(-40.0), 0.0);
        assert_eq!(normal_cdf(40.0), 1.0);
    }

    #[test]
    fn the_quantile_matches_published_critical_values() {
        // Φ⁻¹(0.99) = 2.3263478740408408, the constant the expected-maximum
        // formula evaluates at 100 trials.
        assert!((normal_quantile(0.99) - 2.326_347_874_040_841).abs() < 1e-11);
        assert!((normal_quantile(0.975) - 1.959_963_984_540_054).abs() < 1e-11);
        assert_eq!(normal_quantile(0.5), 0.0);
    }

    #[test]
    fn the_quantile_inverts_the_cdf_across_the_range() {
        // Round-tripping is the check that catches a mismatched constant in
        // either direction, which comparing each against its own table would not.
        for step in -60..=60 {
            let z = step as f64 / 10.0;
            let back = normal_quantile(normal_cdf(z));
            assert!(
                (back - z).abs() < 1e-6,
                "round trip of z={z} came back as {back}",
            );
        }
    }

    #[test]
    fn the_quantile_is_infinite_at_the_endpoints_rather_than_clamped() {
        // The deflation evaluates Φ⁻¹(1 - 1/N); at N = 1 that is this value, and
        // a silently clamped finite number there would report a correction where
        // none is computable.
        assert_eq!(normal_quantile(1.0), f64::INFINITY);
        assert_eq!(normal_quantile(0.0), f64::NEG_INFINITY);
        assert!(normal_quantile(1.5).is_nan());
    }

    #[test]
    fn a_symmetric_series_has_zero_skew() {
        let v = vec![-2.0, -1.0, 0.0, 1.0, 2.0];
        assert!(skewness(&v).abs() < 1e-15);
    }

    #[test]
    fn a_left_tailed_series_has_negative_skew() {
        // One large loss among small gains: the shape that lowers the
        // probabilistic Sharpe ratio. Hand check: mean = -0.008, and the cubed
        // deviation of -0.10 dominates.
        let v = vec![0.01, 0.01, 0.01, 0.01, -0.10];
        let s = skewness(&v);
        assert!(s < -1.4, "expected strong negative skew, got {s}");
    }

    #[test]
    fn kurtosis_is_reported_non_excess() {
        // A uniform five-point ladder has population kurtosis 1.7 (excess -1.3).
        // The contrastive assertion is the point: an excess-form implementation
        // would return -1.3 here and shift every deflated Sharpe silently.
        let v = vec![-2.0, -1.0, 0.0, 1.0, 2.0];
        let k = kurtosis(&v);
        assert!((k - 1.7).abs() < 1e-12, "expected 1.7 non-excess, got {k}");
        assert!(k > 0.0, "a non-excess kurtosis is never negative");
    }

    #[test]
    fn fat_tails_raise_the_kurtosis_above_three() {
        let v = vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0];
        assert!(kurtosis(&v) > 3.0);
    }

    #[test]
    fn a_constant_series_has_a_standard_deviation_of_exactly_zero() {
        // Ten copies of 0.02 sum to 0.19999999999999998, so the naive variance is
        // ~1e-36 rather than 0. Callers need `== 0.0` to be a usable test for "no
        // spread", and standardised moments would otherwise amplify the dust into
        // a skewness of 1.0.
        assert_eq!(population_std_dev(&vec![0.02; 10]), 0.0);
        assert_eq!(population_std_dev(&vec![1.4; 400]), 0.0);
        assert_eq!(population_std_dev(&[]), 0.0);
        // A real spread survives, twelve orders of magnitude above the threshold.
        assert!((population_std_dev(&[0.01, -0.01]) - 0.01).abs() < 1e-17);
    }

    #[test]
    fn a_degenerate_series_reports_the_neutral_moments() {
        // No shape to measure, so report the values a normal sample would give
        // rather than a NaN that would propagate into a reported probability.
        for v in [vec![0.02; 10], vec![], vec![0.01]] {
            assert_eq!(skewness(&v), 0.0);
            assert_eq!(kurtosis(&v), 3.0);
        }
    }

    #[test]
    fn sorting_ascends_on_finite_values() {
        let mut v = vec![0.3, -0.1, 0.2, -0.05];
        sort_ascending(&mut v);
        assert_eq!(v, vec![-0.1, -0.05, 0.2, 0.3]);
    }

    #[test]
    fn a_nan_does_not_panic_but_does_not_sort_either() {
        // Documenting the limitation rather than implying NaN is handled: an
        // intransitive comparator can leave *finite* values out of order around
        // the NaN, so the only guarantees are "returns" and "same length".
        let mut v = vec![0.3, f64::NAN, -0.1, 0.2];
        sort_ascending(&mut v);
        assert_eq!(v.len(), 4);
        assert_eq!(v.iter().filter(|x| x.is_nan()).count(), 1);
    }
}
