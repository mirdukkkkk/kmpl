use crate::{BinOp, Expr, Ident, Span, TypeExpr};

pub type Block = Vec<Stmt>;

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

/// One step of an assignment target.
#[derive(Debug, Clone, PartialEq)]
pub enum PathStep {
    Index(Expr),
    Field(Ident),
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
    /// `name[i].x = value`: one step per `[...]` or `.field`. `op` is set
    /// for `name[i] += value`. Not desugared to `a[i] = a[i] + value`,
    /// because the indices must run only once.
    PathAssign {
        name: Ident,
        path: Vec<PathStep>,
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
