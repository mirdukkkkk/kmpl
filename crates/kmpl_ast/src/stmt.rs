use crate::{Expr, Ident, Span, TypeExpr};

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
    Assign { name: Ident, value: Expr },
    /// `name[i][j] = value`: one entry in `indices` per `[...]`.
    IndexAssign {
        name: Ident,
        indices: Vec<Expr>,
        value: Expr,
    },
    If { cond: Expr, then: Block, otherwise: Option<Block> },
    While { cond: Expr, body: Block },
    /// `for var in start..end { body }`: `end` is not included.
    For {
        var: Ident,
        start: Expr,
        end: Expr,
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
