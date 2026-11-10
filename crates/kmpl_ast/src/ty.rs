use crate::Ident;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int,
    Real,
    Bool,
    Complex,
    Vec2,
    Vec3,
    Vec4,
    Str,
    Bytes,
    /// Rows and columns of `real`s.
    Matrix,
}

impl Type {
    /// Every type, in a fixed order.
    pub const ALL: [Type; 10] = [
        Type::Int,
        Type::Real,
        Type::Bool,
        Type::Complex,
        Type::Vec2,
        Type::Vec3,
        Type::Vec4,
        Type::Str,
        Type::Bytes,
        Type::Matrix,
    ];

    /// The type with this name in kmpl source, like `real` or `vec3`.
    pub fn from_name(name: &str) -> Option<Type> {
        let ty = match name {
            "int" => Type::Int,
            "real" => Type::Real,
            "bool" => Type::Bool,
            "complex" => Type::Complex,
            "vec2" => Type::Vec2,
            "vec3" => Type::Vec3,
            "vec4" => Type::Vec4,
            "string" => Type::Str,
            "bytes" => Type::Bytes,
            "matrix" => Type::Matrix,
            _ => return None,
        };
        Some(ty)
    }
}

/// A fully inferred type, as later stages see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Prim(Type),
    Array(Box<Ty>),
    Tuple(Vec<Ty>),
    /// The name of a struct or enum, with its module prefix.
    Struct(String),
    /// `T?`
    Optional(Box<Ty>),
    /// A generic struct or enum with its type arguments: `Pair<int>`.
    Instance { name: String, args: Vec<Ty> },
    /// `fn(int) -> real`
    Function { params: Vec<Ty>, ret: Option<Box<Ty>> },
}

/// A type as written in the source.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Known(Type),
    /// Any other name: a generic parameter like `T`, a struct, or a typo.
    /// The parser cannot tell which; the checker reports unknown names.
    Param(Ident),
    /// `[T]`: an array of `T`, of any length.
    Array(Box<TypeExpr>),
    /// `(A, B)`: at least two types.
    Tuple(Vec<TypeExpr>),
    /// `T?`: a value of type `T`, or nothing.
    Optional(Box<TypeExpr>),
    /// `Pair<int>`: a generic struct or enum with its type arguments.
    Applied { name: Ident, args: Vec<TypeExpr> },
    /// `fn(int) -> real`
    Function { params: Vec<TypeExpr>, ret: Option<Box<TypeExpr>> },
}
