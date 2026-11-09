//! The `matrix` type: rows and columns of `real`s in one block of memory,
//! so `m[i, j]` is one read and a product walks memory in order.

use std::sync::Arc;

use kmpl_ast::BinOp;

use crate::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    /// Row after row.
    pub data: Vec<f64>,
}

impl Matrix {
    /// `matrix(rows, cols)`: every number is zero.
    pub fn zeros(rows: i64, cols: i64) -> Result<Matrix, String> {
        let size = |n: i64, what: &str| {
            usize::try_from(n).map_err(|_| format!("a matrix cannot have {n} {what}"))
        };
        let (row_count, col_count) = (size(rows, "rows")?, size(cols, "columns")?);
        let too_large = || format!("a matrix of {rows} by {cols} is too large");
        let total = row_count.checked_mul(col_count).ok_or_else(too_large)?;
        let mut data = Vec::new();
        data.try_reserve_exact(total).map_err(|_| too_large())?;
        data.resize(total, 0.0);
        Ok(Matrix {
            rows: row_count,
            cols: col_count,
            data,
        })
    }

    /// From a `[[real]]`, whose rows must have one length.
    pub fn from_rows(value: &Value) -> Result<Matrix, String> {
        let rows = value.as_array();
        let cols = rows.first().map_or(0, |row| row.as_reals().len());
        let mut data = Vec::with_capacity(rows.len() * cols);
        for row in rows {
            let row = row.as_reals();
            if row.len() != cols {
                return Err("the rows of this matrix have different lengths".to_owned());
            }
            data.extend_from_slice(row);
        }
        Ok(Matrix {
            rows: rows.len(),
            cols,
            data,
        })
    }

    /// As a `[[real]]`.
    pub fn to_rows(&self) -> Value {
        let rows = self
            .row_slices()
            .map(|row| Value::Reals(Arc::new(row.to_vec())))
            .collect();
        Value::Array(Arc::new(rows))
    }

    /// Every row, also when rows have no numbers in them.
    pub fn row_slices(&self) -> impl Iterator<Item = &[f64]> {
        (0..self.rows).map(|i| &self.data[i * self.cols..(i + 1) * self.cols])
    }

    pub fn transposed(&self) -> Matrix {
        let mut data = Vec::with_capacity(self.data.len());
        for j in 0..self.cols {
            for i in 0..self.rows {
                data.push(self.data[i * self.cols + j]);
            }
        }
        Matrix {
            rows: self.cols,
            cols: self.rows,
            data,
        }
    }

    fn map(&self, f: impl Fn(f64) -> f64) -> Matrix {
        Matrix {
            rows: self.rows,
            cols: self.cols,
            data: self.data.iter().map(|x| f(*x)).collect(),
        }
    }

    fn zip(&self, other: &Matrix, f: impl Fn(f64, f64) -> f64) -> Result<Matrix, String> {
        if (self.rows, self.cols) != (other.rows, other.cols) {
            return Err(format!(
                "matrices have different shapes: {}x{} and {}x{}",
                self.rows, self.cols, other.rows, other.cols
            ));
        }
        let data = self.data.iter().zip(&other.data).map(|(x, y)| f(*x, *y));
        Ok(Matrix {
            rows: self.rows,
            cols: self.cols,
            data: data.collect(),
        })
    }

    /// Terms are added in the order of the columns of `self`, but row by
    /// row, which is much kinder to the cache than column by column.
    fn product(&self, other: &Matrix) -> Result<Matrix, String> {
        if self.cols != other.rows {
            return Err(format!(
                "cannot multiply a {}x{} matrix by a {}x{} matrix",
                self.rows, self.cols, other.rows, other.cols
            ));
        }
        let width = other.cols;
        let mut data = vec![0.0; self.rows * width];
        if self.cols == 0 || width == 0 {
            return Ok(Matrix {
                rows: self.rows,
                cols: width,
                data,
            });
        }
        for (row, entries) in self.row_slices().zip(data.chunks_mut(width)) {
            for (entry, q) in entries.iter_mut().zip(&other.data[..width]) {
                *entry = row[0] * q;
            }
            for (p, other_row) in row.iter().zip(other.row_slices()).skip(1) {
                for (entry, q) in entries.iter_mut().zip(other_row) {
                    *entry += p * q;
                }
            }
        }
        Ok(Matrix {
            rows: self.rows,
            cols: width,
            data,
        })
    }

    fn times_vector(&self, vector: &[f64]) -> Result<Vec<f64>, String> {
        if self.cols != vector.len() {
            return Err(format!(
                "cannot multiply a {}x{} matrix by a vector of length {}",
                self.rows,
                self.cols,
                vector.len()
            ));
        }
        let dot = |row: &[f64]| {
            let mut terms = row.iter().zip(vector).map(|(p, q)| p * q);
            let first = terms.next().unwrap_or(0.0);
            terms.fold(first, |total, term| total + term)
        };
        Ok(self.row_slices().map(dot).collect())
    }
}

/// How `print` shows a matrix: like the `[[real]]` with the same numbers.
impl std::fmt::Display for Matrix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rows: Vec<String> = self
            .row_slices()
            .map(|row| {
                let numbers: Vec<String> = row.iter().map(|x| format!("{x:?}")).collect();
                format!("[{}]", numbers.join(", "))
            })
            .collect();
        write!(f, "[{}]", rows.join(", "))
    }
}

fn wrap(matrix: Matrix) -> Value {
    Value::Matrix(Arc::new(matrix))
}

pub(crate) fn negated(matrix: &Matrix) -> Value {
    wrap(matrix.map(|x| -x))
}

/// An operator with a matrix on at least one side. `None` for a combination
/// the checker should have rejected.
pub(crate) fn binary(op: BinOp, l: &Value, r: &Value) -> Result<Option<Value>, String> {
    use Value::{Matrix, Real, Reals};

    Ok(Some(match (op, l, r) {
        (BinOp::Add, Matrix(a), Matrix(b)) => wrap(a.zip(b, |x, y| x + y)?),
        (BinOp::Sub, Matrix(a), Matrix(b)) => wrap(a.zip(b, |x, y| x - y)?),
        (BinOp::Mul, Matrix(a), Matrix(b)) => wrap(a.product(b)?),
        (BinOp::Mul, Matrix(a), Reals(v)) => Reals(Arc::new(a.times_vector(v)?)),
        (BinOp::Mul, Matrix(a), Real(k)) | (BinOp::Mul, Real(k), Matrix(a)) => {
            wrap(a.map(|x| x * k))
        }
        (BinOp::Div, Matrix(a), Real(k)) => wrap(a.map(|x| x / k)),
        _ => return Ok(None),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(rows: usize, cols: usize, data: &[f64]) -> Matrix {
        Matrix {
            rows,
            cols,
            data: data.to_vec(),
        }
    }

    #[test]
    fn products() {
        let a = matrix(2, 3, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let b = matrix(3, 2, &[7.0, 8.0, 9.0, 10.0, 11.0, 12.0]);
        assert_eq!(
            a.product(&b).unwrap(),
            matrix(2, 2, &[58.0, 64.0, 139.0, 154.0])
        );
        assert_eq!(
            a.product(&a).unwrap_err(),
            "cannot multiply a 2x3 matrix by a 2x3 matrix"
        );
        assert_eq!(a.times_vector(&[1.0, 0.0, -1.0]).unwrap(), [-2.0, -2.0]);
        assert_eq!(
            a.times_vector(&[1.0]).unwrap_err(),
            "cannot multiply a 2x3 matrix by a vector of length 1"
        );
    }

    #[test]
    fn empty_shapes() {
        let tall = matrix(2, 0, &[]);
        let wide = matrix(0, 3, &[]);
        assert_eq!(tall.product(&wide).unwrap(), matrix(2, 3, &[0.0; 6]));
        assert_eq!(wide.product(&matrix(3, 0, &[])).unwrap(), matrix(0, 0, &[]));
        assert_eq!(tall.times_vector(&[]).unwrap(), [0.0, 0.0]);
        assert_eq!(tall.to_string(), "[[], []]");
        assert_eq!(wide.to_string(), "[]");
    }

    #[test]
    fn rows_there_and_back() {
        let m = matrix(2, 2, &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(Matrix::from_rows(&m.to_rows()).unwrap(), m);
        assert_eq!(m.to_string(), "[[1.0, 2.0], [3.0, 4.0]]");
        let ragged = Value::Array(Arc::new(vec![
            Value::Reals(Arc::new(vec![1.0])),
            Value::Reals(Arc::new(vec![])),
        ]));
        assert!(
            Matrix::from_rows(&ragged)
                .unwrap_err()
                .contains("different lengths")
        );
    }

    #[test]
    fn sizes() {
        assert_eq!(Matrix::zeros(2, 3).unwrap(), matrix(2, 3, &[0.0; 6]));
        assert_eq!(
            Matrix::zeros(-1, 3).unwrap_err(),
            "a matrix cannot have -1 rows"
        );
        assert_eq!(
            Matrix::zeros(1, -2).unwrap_err(),
            "a matrix cannot have -2 columns"
        );
        assert!(
            Matrix::zeros(i64::MAX, 2)
                .unwrap_err()
                .contains("too large")
        );
    }

    #[test]
    fn shapes_must_match_to_add() {
        let a = matrix(1, 2, &[1.0, 2.0]);
        let b = matrix(2, 1, &[1.0, 2.0]);
        assert_eq!(
            a.zip(&b, |x, y| x + y).unwrap_err(),
            "matrices have different shapes: 1x2 and 2x1"
        );
    }
}
