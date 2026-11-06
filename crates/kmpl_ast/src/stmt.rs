use crate::{Expr, Ident, Span, Type};

pub type Block = Vec<Stmt>;

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Let { name: Ident, ty: Option<Type>, init: Option<Expr> },
    Assign { name: Ident, value: Expr },
    If { cond: Expr, then: Block, otherwise: Option<Block> },
    While { cond: Expr, body: Block },
    Break,
    Continue,
    Return(Option<Expr>),
    Expr(Expr),
}
