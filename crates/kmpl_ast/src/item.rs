use crate::{Block, Expr, Ident, Span, Type, TypeExpr};

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Const(Const),
    Function(Function),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Const {
    pub name: Ident,
    pub ty: TypeExpr,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: Ident,
    /// `<T: float | vector, U>`. Empty for an ordinary function.
    pub generics: Vec<Generic>,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub ty: TypeExpr,
}

/// A generic parameter: `T: float | vector`.
#[derive(Debug, Clone, PartialEq)]
pub struct Generic {
    pub name: Ident,
    /// The types `T` may be. Every type when no bound is written.
    pub bound: Vec<Type>,
}
