use kmpl_ast::Type;

/// A runtime value. One variant per `Type`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i32),
    Real(f32),
    Bool(bool),
    Complex { re: f32, im: f32 },
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
}

impl Value {
    pub fn ty(&self) -> Type {
        match self {
            Value::Int(_) => Type::Int,
            Value::Real(_) => Type::Real,
            Value::Bool(_) => Type::Bool,
            Value::Complex { .. } => Type::Complex,
            Value::Vec2(_) => Type::Vec2,
            Value::Vec3(_) => Type::Vec3,
            Value::Vec4(_) => Type::Vec4,
        }
    }

    /// The inner `f32` of a `Real`.
    ///
    /// Panics on other variants. The checker guarantees argument types
    /// before the interpreter calls a built-in, so a panic here is a bug
    /// in the checker, not in the user's program.
    pub fn as_real(&self) -> f32 {
        match self {
            Value::Real(v) => *v,
            other => panic!("expected real, got {:?}", other.ty()),
        }
    }
}
