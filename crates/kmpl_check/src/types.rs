//! Type variables and unification.
//!
//! Every expression gets a type variable. A variable holds the set of
//! types it may still become: the literal `1` starts as
//! `{int, real, complex}`, an annotation `real` is `{real}`.
//! Unifying two variables intersects their sets and merges them into one;
//! an empty intersection is a type error.

use kmpl_ast::Type;

/// A set of `Type`s, one bit per type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeSet(u8);

/// All types, in the order used for defaults and for messages.
const ALL: [Type; 7] = [
    Type::Int,
    Type::Real,
    Type::Complex,
    Type::Bool,
    Type::Vec2,
    Type::Vec3,
    Type::Vec4,
];

impl TypeSet {
    pub const ANY: TypeSet = TypeSet(0b111_1111);
    /// Type of an integer literal like `1`.
    pub const NUMBER: TypeSet = TypeSet::of(&[Type::Int, Type::Real, Type::Complex]);
    /// Type of a real literal like `1.0`.
    pub const FLOAT: TypeSet = TypeSet::of(&[Type::Real, Type::Complex]);
    /// Types that `<`, `<=`, `>`, `>=` work on.
    pub const ORDERED: TypeSet = TypeSet::of(&[Type::Int, Type::Real]);
    pub const VECTOR: TypeSet = TypeSet::of(&[Type::Vec2, Type::Vec3, Type::Vec4]);
    /// Types that `+`, `-` and unary `-` work on.
    pub const LINEAR: TypeSet = TypeSet(TypeSet::NUMBER.0 | TypeSet::VECTOR.0);

    const fn bit(ty: Type) -> u8 {
        1 << ty as u8
    }

    pub const fn single(ty: Type) -> TypeSet {
        TypeSet(TypeSet::bit(ty))
    }

    pub const fn of(types: &[Type]) -> TypeSet {
        let mut bits = 0;
        let mut i = 0;
        while i < types.len() {
            bits |= TypeSet::bit(types[i]);
            i += 1;
        }
        TypeSet(bits)
    }

    pub fn contains(self, ty: Type) -> bool {
        self.0 & TypeSet::bit(ty) != 0
    }

    pub fn intersect(self, other: TypeSet) -> TypeSet {
        TypeSet(self.0 & other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// `true` if every type in `self` is also in `other`.
    pub fn is_subset(self, other: TypeSet) -> bool {
        self.0 & !other.0 == 0
    }

    /// The type, if exactly one is left.
    pub fn as_single(self) -> Option<Type> {
        match self.types().as_slice() {
            [ty] => Some(*ty),
            _ => None,
        }
    }

    /// The type to pick when inference ends with several candidates:
    /// `int`, then `real`, then `complex`. Only number sets have a default;
    /// anything else (`{vec2, vec3}`, `ANY`) needs an annotation.
    pub fn default_type(self) -> Option<Type> {
        if let Some(ty) = self.as_single() {
            return Some(ty);
        }
        if self.is_empty() || !self.is_subset(TypeSet::NUMBER) {
            return None;
        }
        self.types().first().copied()
    }

    fn types(self) -> Vec<Type> {
        ALL.into_iter().filter(|&ty| self.contains(ty)).collect()
    }
}

/// For messages: "`real`", "`real` or `complex`", "`int`, `real` or `complex`".
impl std::fmt::Display for TypeSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == TypeSet::ANY {
            return write!(f, "any type");
        }

        let names: Vec<String> = self
            .types()
            .iter()
            .map(|ty| format!("`{}`", type_name(*ty)))
            .collect();

        match names.as_slice() {
            [] => write!(f, "no type"),
            [one] => write!(f, "{one}"),
            [rest @ .., last] => write!(f, "{} or {last}", rest.join(", ")),
        }
    }
}

/// The name of a type as written in kmpl source.
pub fn type_name(ty: Type) -> &'static str {
    match ty {
        Type::Int => "int",
        Type::Real => "real",
        Type::Bool => "bool",
        Type::Complex => "complex",
        Type::Vec2 => "vec2",
        Type::Vec3 => "vec3",
        Type::Vec4 => "vec4",
    }
}

/// Handle to a type variable inside a `Unifier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TyVar(usize);

enum Slot {
    /// Merged into another variable.
    Link(usize),
    /// A representative variable with its set of possible types.
    Root(TypeSet),
}

/// Union-find over type variables.
#[derive(Default)]
pub struct Unifier {
    slots: Vec<Slot>,
}

impl Unifier {
    pub fn new_var(&mut self, set: TypeSet) -> TyVar {
        self.slots.push(Slot::Root(set));
        TyVar(self.slots.len() - 1)
    }

    /// Index of the root slot that `var` is merged into.
    fn root(&self, var: TyVar) -> usize {
        let mut i = var.0;
        while let Slot::Link(next) = self.slots[i] {
            i = next;
        }
        i
    }

    pub fn set(&self, var: TyVar) -> TypeSet {
        match self.slots[self.root(var)] {
            Slot::Root(set) => set,
            Slot::Link(_) => unreachable!("root() returns a root"),
        }
    }

    /// Makes `a` and `b` the same type. On failure nothing changes and
    /// the two sets are returned for the error message.
    pub fn unify(&mut self, a: TyVar, b: TyVar) -> Result<(), (TypeSet, TypeSet)> {
        let (ra, rb) = (self.root(a), self.root(b));
        if ra == rb {
            return Ok(());
        }

        let (sa, sb) = (self.set(a), self.set(b));
        let both = sa.intersect(sb);
        if both.is_empty() {
            return Err((sa, sb));
        }

        self.slots[ra] = Slot::Root(both);
        self.slots[rb] = Slot::Link(ra);
        Ok(())
    }

    /// Narrows `var` to the types in `allowed`. On failure nothing changes
    /// and the current set of `var` is returned.
    pub fn restrict(&mut self, var: TyVar, allowed: TypeSet) -> Result<(), TypeSet> {
        let current = self.set(var);
        let narrowed = current.intersect(allowed);
        if narrowed.is_empty() {
            return Err(current);
        }

        let root = self.root(var);
        self.slots[root] = Slot::Root(narrowed);
        Ok(())
    }
}
