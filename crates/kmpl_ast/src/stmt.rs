use crate::{BinOp, Expr, ExprKind, Ident, Span, TypeExpr};

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

// A tree lives only while the program is compiled, so the size does not matter.
#[allow(clippy::large_enum_variant)]
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
    /// `match value { pattern => { ... } ... }`
    Match { value: Expr, arms: Vec<MatchArm> },
    Break,
    Continue,
    Return(Option<Expr>),
    Expr(Expr),
}

impl Stmt {
    /// Is this a call to the built-in `panic`, which never comes back?
    pub fn is_panic(&self) -> bool {
        matches!(&self.kind, StmtKind::Expr(Expr { kind: ExprKind::Call { name, .. }, .. })
            if name.name == "panic")
    }
}

impl Pattern {
    /// The variables this pattern creates, possibly with `_` among them.
    pub fn bindings(&self) -> &[Ident] {
        match self {
            Pattern::Variant { bindings, .. } => bindings,
            Pattern::Some(name) => std::slice::from_ref(name),
            Pattern::None | Pattern::Wildcard => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    /// Where the pattern starts, for messages about it.
    pub span: Span,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `Shape::Rect(w, h)`. `name` is the enum. A binding `_` is a value
    /// nobody needs.
    Variant { name: Ident, variant: Ident, bindings: Vec<Ident> },
    /// `some(x)`
    Some(Ident),
    /// `none`
    None,
    /// `_`
    Wildcard,
}
