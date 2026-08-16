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
