use crate::{Block, Ident, Param, Span, TypeExpr};

/// `fn(x: real) -> real { ... }`, a function without a name.
#[derive(Debug, Clone, PartialEq)]
pub struct Lambda {
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Block,
}

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
    /// `"text"`, with escapes already replaced.
    Str(String),
    Var(Ident),
    Unary { op: UnOp, operand: Box<Expr> },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Call { name: Ident, args: Vec<Expr> },
    Swizzle { base: Box<Expr>, mask: Ident },
    /// `(a, b)`: at least two elements. `(a)` is just `a`.
    Tuple(Vec<Expr>),
    /// `[a, b, c]`
    Array(Vec<Expr>),
    /// `[value; len]`: `len` copies of `value`. `len` is known at run time.
    ArrayFill { value: Box<Expr>, len: Box<Expr> },
    /// `base[index]`
    Index { base: Box<Expr>, index: Box<Expr> },
    /// `base.name`
    Field { base: Box<Expr>, name: Ident },
    /// `Point { x: 1.0, y: 2.0 }`, fields in the order they are written.
    StructLit { name: Ident, fields: Vec<(Ident, Expr)> },
    /// `Shape::Circle(1.0)` or `Shape::Empty`. `name` is the enum.
    Variant { name: Ident, variant: Ident, args: Vec<Expr> },
    /// `some(value)`
    Some(Box<Expr>),
    /// `none`
    None,
    Lambda(Box<Lambda>),
    /// `base[start..end]`, a copy of that part. A missing `start` is 0, a
    /// missing `end` is the length. `inclusive` is `..=`.
    Slice {
        base: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Rem,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
    BitAnd, BitOr, BitXor, Shl, Shr,
}
