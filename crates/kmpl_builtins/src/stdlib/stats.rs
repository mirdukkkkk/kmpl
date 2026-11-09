//! `std:stats`.

use std::sync::Arc;

use kmpl_ast::Type;

use crate::{Builtin, ParamType, RetType, Value, ok, pair};

fn require_numbers(data: &[f64], function: &str) -> Result<(), String> {
    if data.is_empty() {
        return Err(format!("`stats.{function}` needs at least one number"));
    }
    Ok(())
}

fn require_two(data: &[f64], function: &str) -> Result<(), String> {
    if data.len() < 2 {
        return Err(format!(
            "`stats.{function}` needs at least two numbers, got {}",
            data.len()
        ));
    }
    Ok(())
}

fn mean(data: &[f64]) -> f64 {
    data.iter().sum::<f64>() / data.len() as f64
}

/// Dividing by `n - 1`: the data is a sample.
fn variance(data: &[f64]) -> f64 {
    let center = mean(data);
    let squares: f64 = data.iter().map(|x| (x - center) * (x - center)).sum();
    squares / (data.len() - 1) as f64
}

/// A sorted copy. `NaN` has no place in an order, so it is refused.
fn sorted(data: &[f64], function: &str) -> Result<Vec<f64>, String> {
    require_numbers(data, function)?;
    if data.iter().any(|x| x.is_nan()) {
        return Err(format!("`stats.{function}` cannot order NaN"));
    }
    let mut copy = data.to_vec();
    copy.sort_unstable_by(f64::total_cmp);
    Ok(copy)
}

/// Between the two nearest numbers of sorted data, in proportion.
fn quantile_of_sorted(sorted: &[f64], q: f64) -> f64 {
    let position = q * (sorted.len() - 1) as f64;
    let below = position.floor() as usize;
    let above = position.ceil() as usize;
    sorted[below] + (sorted[above] - sorted[below]) * (position - below as f64)
}

/// The sums of squares and of products around the means, and the means.
#[derive(Debug)]
struct Paired {
    mean_x: f64,
    mean_y: f64,
    xx: f64,
    yy: f64,
    xy: f64,
}

fn paired(x: &[f64], y: &[f64], function: &str) -> Result<Paired, String> {
    if x.len() != y.len() {
        return Err(format!(
            "`stats.{function}` got {} and {} numbers, they must be equal",
            x.len(),
            y.len()
        ));
    }
    require_two(x, function)?;

    let (mean_x, mean_y) = (mean(x), mean(y));
    let mut sums = Paired {
        mean_x,
        mean_y,
        xx: 0.0,
        yy: 0.0,
        xy: 0.0,
    };
    for (a, b) in x.iter().zip(y) {
        sums.xx += (a - mean_x) * (a - mean_x);
        sums.yy += (b - mean_y) * (b - mean_y);
        sums.xy += (a - mean_x) * (b - mean_y);
    }
    Ok(sums)
}

fn histogram(data: &[f64], bins: i64) -> Result<Vec<i64>, String> {
    require_numbers(data, "histogram")?;
    if bins < 1 {
        return Err(format!(
            "`stats.histogram` needs at least one bin, got {bins}"
        ));
    }
    if data.iter().any(|x| !x.is_finite()) {
        return Err("`stats.histogram` needs finite numbers".to_owned());
    }

    let size = usize::try_from(bins).map_err(|_| too_many_bins(bins))?;
    let mut counts = Vec::new();
    counts
        .try_reserve_exact(size)
        .map_err(|_| too_many_bins(bins))?;
    counts.resize(size, 0);

    let low = data.iter().copied().fold(f64::INFINITY, f64::min);
    let high = data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let width = (high - low) / bins as f64;
    for x in data {
        // All numbers equal gives a width of zero; they go in the first bin.
        let bin = match width > 0.0 {
            true => ((x - low) / width) as usize,
            false => 0,
        };
        // The largest number is the end of the last bin, not outside it.
        counts[bin.min(size - 1)] += 1;
    }
    Ok(counts)
}

fn too_many_bins(bins: i64) -> String {
    format!("`stats.histogram` cannot make {bins} bins")
}

pub(super) static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "_stats_mean",
        generic: &[],
        params: &[ParamType::Reals],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            require_numbers(a[0].as_reals(), "mean")?;
            ok(Value::Real(mean(a[0].as_reals())))
        },
    },
    Builtin {
        name: "_stats_median",
        generic: &[],
        params: &[ParamType::Reals],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            let data = sorted(a[0].as_reals(), "median")?;
            ok(Value::Real(quantile_of_sorted(&data, 0.5)))
        },
    },
    Builtin {
        name: "_stats_variance",
        generic: &[],
        params: &[ParamType::Reals],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            require_two(a[0].as_reals(), "variance")?;
            ok(Value::Real(variance(a[0].as_reals())))
        },
    },
    Builtin {
        name: "_stats_stddev",
        generic: &[],
        params: &[ParamType::Reals],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            require_two(a[0].as_reals(), "stddev")?;
            ok(Value::Real(variance(a[0].as_reals()).sqrt()))
        },
    },
    Builtin {
        name: "_stats_quantile",
        generic: &[],
        params: &[ParamType::Reals, ParamType::Exact(Type::Real)],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            let q = a[1].as_real();
            if !(0.0..=1.0).contains(&q) {
                return Err(format!(
                    "`stats.quantile` needs a number from 0 to 1, got {q:?}"
                ));
            }
            let data = sorted(a[0].as_reals(), "quantile")?;
            ok(Value::Real(quantile_of_sorted(&data, q)))
        },
    },
    Builtin {
        name: "_stats_correlation",
        generic: &[],
        params: &[ParamType::Reals, ParamType::Reals],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            let sums = paired(a[0].as_reals(), a[1].as_reals(), "correlation")?;
            if sums.xx == 0.0 || sums.yy == 0.0 {
                return Err("`stats.correlation` needs numbers that differ".to_owned());
            }
            ok(Value::Real(sums.xy / (sums.xx * sums.yy).sqrt()))
        },
    },
    Builtin {
        name: "_stats_histogram",
        generic: &[],
        params: &[ParamType::Reals, ParamType::Exact(Type::Int)],
        ret: RetType::ArrayOf(Type::Int),
        func: |a, _| {
            let counts = histogram(a[0].as_reals(), a[1].as_int())?;
            ok(Value::Ints(Arc::new(counts)))
        },
    },
    Builtin {
        name: "_stats_linear_fit",
        generic: &[],
        params: &[ParamType::Reals, ParamType::Reals],
        ret: RetType::Pair(Type::Real, Type::Real),
        func: |a, _| {
            let sums = paired(a[0].as_reals(), a[1].as_reals(), "linear_fit")?;
            if sums.xx == 0.0 {
                return Err("`stats.linear_fit` needs `x` values that differ".to_owned());
            }
            let slope = sums.xy / sums.xx;
            let intercept = sums.mean_y - slope * sums.mean_x;
            ok(pair(Value::Real(slope), Value::Real(intercept)))
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn centers_and_spreads() {
        let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert_eq!(mean(&data), 5.0);
        assert!(close(variance(&data), 32.0 / 7.0));
        assert_eq!(variance(&[1.0, 3.0]), 2.0);
    }

    #[test]
    fn quantiles_interpolate() {
        let data = sorted(&[4.0, 1.0, 3.0, 2.0], "median").unwrap();
        assert_eq!(quantile_of_sorted(&data, 0.5), 2.5);
        assert_eq!(quantile_of_sorted(&data, 0.0), 1.0);
        assert_eq!(quantile_of_sorted(&data, 1.0), 4.0);
        assert!(close(quantile_of_sorted(&data, 0.25), 1.75));
        assert_eq!(quantile_of_sorted(&[7.0], 0.9), 7.0);
    }

    #[test]
    fn nothing_to_order_is_an_error() {
        assert!(sorted(&[], "median").unwrap_err().contains("at least one"));
        assert!(
            sorted(&[1.0, f64::NAN], "median")
                .unwrap_err()
                .contains("NaN")
        );
    }

    #[test]
    fn a_line_is_found_exactly() {
        let sums = paired(&[1.0, 2.0, 3.0], &[5.0, 7.0, 9.0], "linear_fit").unwrap();
        let slope = sums.xy / sums.xx;
        assert!(close(slope, 2.0));
        assert!(close(sums.mean_y - slope * sums.mean_x, 3.0));
        assert!(close(sums.xy / (sums.xx * sums.yy).sqrt(), 1.0));
    }

    #[test]
    fn paired_data_must_match() {
        assert!(
            paired(&[1.0], &[1.0, 2.0], "c")
                .unwrap_err()
                .contains("equal")
        );
        assert!(
            paired(&[1.0], &[1.0], "c")
                .unwrap_err()
                .contains("at least two")
        );
    }

    #[test]
    fn histograms_cover_everything() {
        assert_eq!(histogram(&[0.0, 1.0, 2.0, 3.0, 4.0], 2).unwrap(), [2, 3]);
        assert_eq!(histogram(&[0.0, 10.0], 5).unwrap(), [1, 0, 0, 0, 1]);
        assert_eq!(histogram(&[3.0, 3.0], 4).unwrap(), [2, 0, 0, 0]);
        assert_eq!(histogram(&[1.0], 1).unwrap(), [1]);
    }

    #[test]
    fn bad_histograms_are_errors() {
        assert!(
            histogram(&[1.0], 0)
                .unwrap_err()
                .contains("at least one bin")
        );
        assert!(
            histogram(&[f64::INFINITY], 2)
                .unwrap_err()
                .contains("finite")
        );
        assert!(histogram(&[f64::NAN], 2).unwrap_err().contains("finite"));
        assert!(
            histogram(&[1.0], i64::MAX)
                .unwrap_err()
                .contains("cannot make")
        );
    }
}
