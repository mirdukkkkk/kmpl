use crate::{Block, Expr, Ident, Span, Type, TypeExpr};

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub imports: Vec<Import>,
    pub items: Vec<Item>,
}

/// `import fs from "std:fs";` or `import (read, write as save) from "std:fs";`
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub names: ImportNames,
    /// The text after `from`.
    pub source: String,
    pub source_span: Span,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportNames {
    /// The whole module under this name: `fs.read`.
    Module(Ident),
    /// These items, used without a prefix.
    Items(Vec<ImportedItem>),
}

/// `read`, or `read as load`. Without `as` both are the same name.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedItem {
    pub name: Ident,
    pub alias: Ident,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Const(Const),
    Function(Function),
    Struct(StructDef),
    Enum(EnumDef),
}

/// `enum Shape { Circle(real), Empty }`: at least one variant.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: Ident,
    /// `<T, E>`. Empty for an ordinary enum.
    pub generics: Vec<Ident>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Ident,
    /// The types of the values it carries, possibly none.
    pub fields: Vec<TypeExpr>,
}

/// `struct Point { x: real, y: real }`
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: Ident,
    /// `<T, E>`. Empty for an ordinary struct.
    pub generics: Vec<Ident>,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: Ident,
    pub ty: TypeExpr,
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
