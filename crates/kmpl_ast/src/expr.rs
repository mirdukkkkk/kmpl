use crate::{Ident, Span};

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Real(f64),
    /// `2.0i` parses as `Complex { re: 0.0, im: 2.0 }`. The checker also
    /// rewrites `1` or `1.0` into this when they are used as `complex`.
    Complex { re: f64, im: f64 },
    Bool(bool),
    Var(Ident),
    Unary { op: UnOp, operand: Box<Expr> },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Call { name: Ident, args: Vec<Expr> },
    Swizzle { base: Box<Expr>, mask: Ident },
    /// `[a, b, c]`
    Array(Vec<Expr>),
    /// `[value; len]`: `len` copies of `value`. `len` is known at run time.
    ArrayFill { value: Box<Expr>, len: Box<Expr> },
    /// `base[index]`
    Index { base: Box<Expr>, index: Box<Expr> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
}
