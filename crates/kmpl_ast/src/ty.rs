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
}

impl Type {
    /// Every type, in a fixed order.
    pub const ALL: [Type; 7] = [
        Type::Int,
        Type::Real,
        Type::Bool,
        Type::Complex,
        Type::Vec2,
        Type::Vec3,
        Type::Vec4,
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
            _ => return None,
        };
        Some(ty)
    }
}

/// A type as written in the source.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Known(Type),
    /// Any other name: a generic parameter like `T`, or a typo. The parser
    /// cannot tell which; the checker reports unknown names.
    Param(Ident),
}

impl TypeExpr {
    /// The type, if it is a built-in one.
    pub fn known(&self) -> Option<Type> {
        match self {
            TypeExpr::Known(ty) => Some(*ty),
            TypeExpr::Param(_) => None,
        }
    }
}
