use crate::{BinOp, Expr, Ident, Span, TypeExpr};

pub type Block = Vec<Stmt>;

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Let {
        name: Ident,
        ty: Option<TypeExpr>,
        init: Option<Expr>,
    },
    /// `let (a, b) = value;`. A name `_` is an element nobody needs.
    LetTuple { names: Vec<Ident>, value: Expr },
    /// `name = value`, or `name += value` when `op` is set.
    Assign {
        name: Ident,
        op: Option<BinOp>,
        value: Expr,
    },
    /// `name[i][j] = value`: one entry in `indices` per `[...]`. `op` is
    /// set for `name[i] += value`. Not desugared to `a[i] = a[i] + value`,
    /// because the indices must run only once.
    IndexAssign {
        name: Ident,
        indices: Vec<Expr>,
        op: Option<BinOp>,
        value: Expr,
    },
    If { cond: Expr, then: Block, otherwise: Option<Block> },
    While { cond: Expr, body: Block },
    /// `for var in start..end step s { body }`. `inclusive` is `..=`;
    /// without it `end` is not included. No `step` means 1.
    For {
        var: Ident,
        start: Expr,
        end: Expr,
        inclusive: bool,
        step: Option<Expr>,
        body: Block,
    },
    /// `for var in array { body }`
    ForEach {
        var: Ident,
        array: Expr,
        body: Block,
    },
    Break,
    Continue,
    Return(Option<Expr>),
    Expr(Expr),
}
