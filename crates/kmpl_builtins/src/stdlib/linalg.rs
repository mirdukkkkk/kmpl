//! `std:linalg`, on the built-in `matrix`.

use std::sync::Arc;

use kmpl_ast::Type;

use crate::ops::checked_line;
use crate::{Builtin, Matrix, ParamType, RetType, Value, ok, optional};

const MATRIX: ParamType = ParamType::Exact(Type::Matrix);

fn square<'a>(value: &'a Value, function: &str) -> Result<&'a Matrix, String> {
    let matrix = value.as_matrix();
    if matrix.rows != matrix.cols {
        return Err(format!(
            "`linalg.{function}` needs a square matrix, this one is {} by {}",
            matrix.rows, matrix.cols
        ));
    }
    Ok(matrix)
}

fn wrap(matrix: Matrix) -> Value {
    Value::Matrix(Arc::new(matrix))
}

/// `PA = LU`, with partial pivoting. `L` has ones on its diagonal, which are
/// not stored; the rest of both is in `data`.
struct Lu {
    size: usize,
    data: Vec<f64>,
    /// `P`: which row of `A` each row came from.
    order: Vec<usize>,
    odd_swaps: bool,
    /// The largest size of a number in `A`.
    scale: f64,
}

impl Lu {
    fn new(matrix: &Matrix) -> Lu {
        let n = matrix.rows;
        let scale = matrix.data.iter().fold(0.0f64, |m, x| m.max(x.abs()));
        let mut data = matrix.data.clone();
        let mut order: Vec<usize> = (0..n).collect();
        let mut odd_swaps = false;

        for k in 0..n {
            let pivot = (k..n)
                .max_by(|a, b| data[a * n + k].abs().total_cmp(&data[b * n + k].abs()))
                .expect("the range starts at k, which is below n");
            if pivot != k {
                for j in 0..n {
                    data.swap(k * n + j, pivot * n + j);
                }
                order.swap(k, pivot);
                odd_swaps = !odd_swaps;
            }

            let divisor = data[k * n + k];
            if divisor == 0.0 {
                continue;
            }
            for i in k + 1..n {
                let factor = data[i * n + k] / divisor;
                data[i * n + k] = factor;
                for j in k + 1..n {
                    data[i * n + j] -= factor * data[k * n + j];
                }
            }
        }

        Lu {
            size: n,
            data,
            order,
            odd_swaps,
            scale,
        }
    }

    fn diagonal(&self) -> impl Iterator<Item = f64> + '_ {
        (0..self.size).map(|i| self.data[i * self.size + i])
    }

    fn determinant(&self) -> f64 {
        let product: f64 = self.diagonal().product();
        if self.odd_swaps { -product } else { product }
    }

    /// Is a pivot so small, next to the numbers of the matrix, that it could
    /// be nothing but rounding error? NaN and infinity count as well.
    fn is_singular(&self) -> bool {
        let tolerance = self.size as f64 * f64::EPSILON * self.scale;
        !tolerance.is_finite() || self.diagonal().any(|d| d.is_nan() || d.abs() <= tolerance)
    }

    /// `x` such that `Ax = b`. The matrix must not be singular.
    fn solve(&self, b: &[f64]) -> Vec<f64> {
        let n = self.size;
        let mut x: Vec<f64> = self.order.iter().map(|&i| b[i]).collect();
        for i in 0..n {
            for j in 0..i {
                x[i] -= self.data[i * n + j] * x[j];
            }
        }
        for i in (0..n).rev() {
            for j in i + 1..n {
                x[i] -= self.data[i * n + j] * x[j];
            }
            x[i] /= self.data[i * n + i];
        }
        x
    }

    fn inverse(&self) -> Matrix {
        let n = self.size;
        let mut columns = Matrix {
            rows: n,
            cols: n,
            data: Vec::with_capacity(n * n),
        };
        let mut unit = vec![0.0; n];
        for j in 0..n {
            unit[j] = 1.0;
            columns.data.extend(self.solve(&unit));
            unit[j] = 0.0;
        }
        // One solution per column was stored as a row.
        columns.transposed()
    }
}

fn identity(n: i64) -> Result<Matrix, String> {
    let too_large = || format!("an identity matrix of size {n} is too large");
    let size = usize::try_from(n).map_err(|_| format!("a matrix cannot have size {n}"))?;
    let total = size.checked_mul(size).ok_or_else(too_large)?;
    let mut data = Vec::new();
    data.try_reserve_exact(total).map_err(|_| too_large())?;
    data.resize(total, 0.0);
    for i in 0..size {
        data[i * size + i] = 1.0;
    }
    Ok(Matrix {
        rows: size,
        cols: size,
        data,
    })
}

pub(super) static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "_linalg_solve",
        generic: &[],
        params: &[MATRIX, ParamType::Reals],
        ret: RetType::OptionalReals,
        func: |a, _| {
            let matrix = square(&a[0], "solve")?;
            let b = a[1].as_reals();
            if b.len() != matrix.rows {
                return Err(format!(
                    "`linalg.solve` got a matrix with {} rows and a vector of {} numbers",
                    matrix.rows,
                    b.len()
                ));
            }
            let lu = Lu::new(matrix);
            ok(optional(
                (!lu.is_singular()).then(|| Value::Reals(Arc::new(lu.solve(b)))),
            ))
        },
    },
    Builtin {
        name: "_linalg_det",
        generic: &[],
        params: &[MATRIX],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            let matrix = square(&a[0], "det")?;
            ok(Value::Real(Lu::new(matrix).determinant()))
        },
    },
    Builtin {
        name: "_linalg_inverse",
        generic: &[],
        params: &[MATRIX],
        ret: RetType::Optional(Type::Matrix),
        func: |a, _| {
            let lu = Lu::new(square(&a[0], "inverse")?);
            ok(optional((!lu.is_singular()).then(|| wrap(lu.inverse()))))
        },
    },
    Builtin {
        name: "_linalg_transpose",
        generic: &[],
        params: &[MATRIX],
        ret: RetType::Exact(Type::Matrix),
        func: |a, _| ok(wrap(a[0].as_matrix().transposed())),
    },
    Builtin {
        name: "_linalg_identity",
        generic: &[],
        params: &[ParamType::Exact(Type::Int)],
        ret: RetType::Exact(Type::Matrix),
        func: |a, _| ok(wrap(identity(a[0].as_int())?)),
    },
    Builtin {
        name: "_linalg_trace",
        generic: &[],
        params: &[MATRIX],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            let matrix = square(&a[0], "trace")?;
            let sum = (0..matrix.rows).map(|i| matrix.data[i * matrix.rows + i]);
            ok(Value::Real(sum.sum()))
        },
    },
    Builtin {
        name: "_linalg_norm",
        generic: &[],
        params: &[MATRIX],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            let matrix = a[0].as_matrix();
            let squares: f64 = matrix.data.iter().map(|x| x * x).sum();
            ok(Value::Real(squares.sqrt()))
        },
    },
    Builtin {
        name: "_linalg_row",
        generic: &[],
        params: &[MATRIX, ParamType::Exact(Type::Int)],
        ret: RetType::ArrayOf(Type::Real),
        func: |a, _| {
            let matrix = a[0].as_matrix();
            let i = checked_line(a[1].as_int(), matrix.rows, "row")?;
            let row = &matrix.data[i * matrix.cols..(i + 1) * matrix.cols];
            ok(Value::Reals(Arc::new(row.to_vec())))
        },
    },
    Builtin {
        name: "_linalg_col",
        generic: &[],
        params: &[MATRIX, ParamType::Exact(Type::Int)],
        ret: RetType::ArrayOf(Type::Real),
        func: |a, _| {
            let matrix = a[0].as_matrix();
            let j = checked_line(a[1].as_int(), matrix.cols, "column")?;
            let column = matrix.row_slices().map(|row| row[j]).collect();
            ok(Value::Reals(Arc::new(column)))
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn dense(rows: usize, cols: usize, data: &[f64]) -> Matrix {
        Matrix {
            rows,
            cols,
            data: data.to_vec(),
        }
    }

    fn close(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn determinants() {
        let det = |m: Matrix| Lu::new(&m).determinant();
        assert!((det(dense(2, 2, &[1.0, 2.0, 3.0, 4.0])) + 2.0).abs() < 1e-12);
        // Needs a row swap, which flips the sign.
        assert_eq!(det(dense(2, 2, &[0.0, 1.0, 1.0, 0.0])), -1.0);
        assert_eq!(det(dense(2, 2, &[1.0, 2.0, 2.0, 4.0])), 0.0);
        assert_eq!(det(dense(0, 0, &[])), 1.0);
    }

    #[test]
    fn solving_needs_pivoting_when_the_first_number_is_zero() {
        let lu = Lu::new(&dense(2, 2, &[0.0, 1.0, 2.0, 0.0]));
        assert!(!lu.is_singular());
        assert!(close(&lu.solve(&[3.0, 4.0]), &[2.0, 3.0]));
    }

    #[test]
    fn a_solution_satisfies_the_system() {
        let a = [4.0, -2.0, 1.0, -2.0, 4.0, -2.0, 1.0, -2.0, 4.0];
        let b = [11.0, -16.0, 17.0];
        let x = Lu::new(&dense(3, 3, &a)).solve(&b);
        let back: Vec<f64> = (0..3)
            .map(|i| (0..3).map(|j| a[i * 3 + j] * x[j]).sum())
            .collect();
        assert!(close(&back, &b), "{back:?}");
    }

    #[test]
    fn singular_matrices_are_noticed() {
        let singular = |m: Matrix| Lu::new(&m).is_singular();
        assert!(singular(dense(2, 2, &[1.0, 2.0, 2.0, 4.0])));
        // Exactly singular, but rounding leaves a tiny pivot.
        assert!(singular(dense(2, 2, &[0.1, 0.2, 0.2, 0.4])));
        assert!(singular(dense(
            3,
            3,
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
        )));
        assert!(singular(dense(1, 1, &[0.0])));
        assert!(singular(dense(1, 1, &[f64::NAN])));
        assert!(singular(dense(1, 1, &[f64::INFINITY])));
        // Small numbers are not the same as a singular matrix.
        assert!(!singular(dense(1, 1, &[1e-20])));
        assert!(!singular(dense(2, 2, &[1e-20, 0.0, 0.0, 1e-20])));
    }

    #[test]
    fn an_inverse_gives_the_identity() {
        let a = [2.0, 1.0, 1.0, 1.0, 3.0, 2.0, 1.0, 0.0, 0.0];
        let inverse = Lu::new(&dense(3, 3, &a)).inverse();
        let mut product = vec![0.0; 9];
        for i in 0..3 {
            for j in 0..3 {
                for k in 0..3 {
                    product[i * 3 + j] += a[i * 3 + k] * inverse.data[k * 3 + j];
                }
            }
        }
        assert!(close(&product, &identity(3).unwrap().data), "{product:?}");
    }

    #[test]
    fn transposing_a_rectangle() {
        assert_eq!(
            dense(2, 3, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).transposed(),
            dense(3, 2, &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0])
        );
    }

    #[test]
    fn identity_sizes() {
        assert_eq!(identity(0).unwrap().data, Vec::<f64>::new());
        assert!(identity(-1).unwrap_err().contains("size -1"));
        assert!(identity(i64::MAX).unwrap_err().contains("too large"));
    }

    #[test]
    fn bad_shapes_are_errors() {
        let wide = wrap(dense(1, 2, &[1.0, 2.0]));
        assert_eq!(
            square(&wide, "det").unwrap_err(),
            "`linalg.det` needs a square matrix, this one is 1 by 2"
        );
        assert_eq!(
            checked_line(2, 2, "row").unwrap_err(),
            "row 2 is out of bounds for a matrix with 2 rows"
        );
    }
}
