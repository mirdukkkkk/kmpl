//! Type variables and unification.
//!
//! Every expression gets a type variable. A variable holds the set of
//! types it may still become: the literal `1` starts as
//! `{int, real, complex}`, an annotation `real` is `{real}`.
//! Unifying two variables intersects their sets and merges them into one;
//! an empty intersection is a type error.
//!
//! The set only covers types from a fixed list. An array is a different
//! kind of variable that points at the variable of its elements, so
//! `[real]` and `[[real]]` need no entries in the list. A variable that may
//! still become an array has the `ARRAY` bit in its set. Tuples work the
//! same way, with the `TUPLE` bit.

use kmpl_ast::Type;

/// A set of `Type`s, one bit per type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeSet(u16);

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
    pub const ANY: TypeSet = TypeSet(0b1_1111_1111);
    /// The set of a variable known to be an array. The bit after the
    /// seven types in `ALL`.
    pub const ARRAY: TypeSet = TypeSet(1 << 7);
    /// The set of a variable known to be a tuple.
    pub const TUPLE: TypeSet = TypeSet(1 << 8);
    /// Type of an integer literal like `1`.
    pub const NUMBER: TypeSet = TypeSet::of(&[Type::Int, Type::Real, Type::Complex]);
    /// Type of a real literal like `1.0`.
    pub const FLOAT: TypeSet = TypeSet::of(&[Type::Real, Type::Complex]);
    /// Types that `<`, `<=`, `>`, `>=` work on.
    pub const ORDERED: TypeSet = TypeSet::of(&[Type::Int, Type::Real]);
    pub const VECTOR: TypeSet = TypeSet::of(&[Type::Vec2, Type::Vec3, Type::Vec4]);
    /// Types that `+`, `-` and unary `-` work on.
    pub const LINEAR: TypeSet = TypeSet(TypeSet::NUMBER.0 | TypeSet::VECTOR.0);

    const fn bit(ty: Type) -> u16 {
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

/// Two types cannot be made equal. The variables are left unchanged, so
/// the caller can still describe them in the message.
#[derive(Debug, PartialEq, Eq)]
pub struct Mismatch;

#[derive(Clone, Copy)]
enum Slot {
    /// Merged into another variable.
    Link(usize),
    /// A representative variable with its set of possible types.
    Root(TypeSet),
    /// A representative variable known to be an array of this element.
    Array(TyVar),
    /// A representative variable known to be a tuple: an index into
    /// `Unifier::tuples`. Keeps `Slot` small and `Copy`.
    Tuple(usize),
}

/// Union-find over type variables.
#[derive(Default)]
pub struct Unifier {
    slots: Vec<Slot>,
    /// The element variables of every tuple.
    tuples: Vec<Vec<TyVar>>,
}

impl Unifier {
    pub fn new_var(&mut self, set: TypeSet) -> TyVar {
        self.slots.push(Slot::Root(set));
        TyVar(self.slots.len() - 1)
    }

    pub fn new_array(&mut self, elem: TyVar) -> TyVar {
        self.slots.push(Slot::Array(elem));
        TyVar(self.slots.len() - 1)
    }

    pub fn new_tuple(&mut self, elems: Vec<TyVar>) -> TyVar {
        self.tuples.push(elems);
        self.slots.push(Slot::Tuple(self.tuples.len() - 1));
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
            Slot::Array(_) => TypeSet::ARRAY,
            Slot::Tuple(_) => TypeSet::TUPLE,
            Slot::Link(_) => unreachable!("root() returns a root"),
        }
    }

    /// The element variable, if `var` is known to be an array.
    pub fn element(&self, var: TyVar) -> Option<TyVar> {
        match self.slots[self.root(var)] {
            Slot::Array(elem) => Some(elem),
            _ => None,
        }
    }

    /// Makes `a` and `b` the same type. On failure nothing changes.
    pub fn unify(&mut self, a: TyVar, b: TyVar) -> Result<(), Mismatch> {
        let (ra, rb) = (self.root(a), self.root(b));
        if ra == rb {
            return Ok(());
        }

        match (self.slots[ra], self.slots[rb]) {
            (Slot::Root(sa), Slot::Root(sb)) => {
                let both = sa.intersect(sb);
                if both.is_empty() {
                    return Err(Mismatch);
                }
                self.slots[ra] = Slot::Root(both);
                self.slots[rb] = Slot::Link(ra);
            }
            // Elements first: if they fail, nothing has changed yet.
            (Slot::Array(ea), Slot::Array(eb)) => {
                self.unify(ea, eb)?;
                self.slots[rb] = Slot::Link(ra);
            }
            (Slot::Tuple(ta), Slot::Tuple(tb)) => {
                self.unify_tuples(ta, tb)?;
                self.slots[rb] = Slot::Link(ra);
            }
            (Slot::Root(set), Slot::Array(_)) => self.bind(ra, set, rb, TypeSet::ARRAY)?,
            (Slot::Array(_), Slot::Root(set)) => self.bind(rb, set, ra, TypeSet::ARRAY)?,
            (Slot::Root(set), Slot::Tuple(_)) => self.bind(ra, set, rb, TypeSet::TUPLE)?,
            (Slot::Tuple(_), Slot::Root(set)) => self.bind(rb, set, ra, TypeSet::TUPLE)?,
            (Slot::Array(_), Slot::Tuple(_)) | (Slot::Tuple(_), Slot::Array(_)) => {
                return Err(Mismatch);
            }
            (Slot::Link(_), _) | (_, Slot::Link(_)) => unreachable!("root() returns a root"),
        }
        Ok(())
    }

    /// Element by element. Earlier elements are already merged when a later
    /// one fails, so the slots are put back to keep "nothing changes".
    fn unify_tuples(&mut self, a: usize, b: usize) -> Result<(), Mismatch> {
        if self.tuples[a].len() != self.tuples[b].len() {
            return Err(Mismatch);
        }

        let saved = self.slots.clone();
        let pairs: Vec<(TyVar, TyVar)> = (self.tuples[a].iter().copied())
            .zip(self.tuples[b].iter().copied())
            .collect();
        for (x, y) in pairs {
            if self.unify(x, y).is_err() {
                self.slots = saved;
                return Err(Mismatch);
            }
        }
        Ok(())
    }

    /// Makes the open variable at `root` the array or tuple at `target`,
    /// whose bit is `kind`. Refuses a variable that would contain itself
    /// (`a = [a]`): no finite type fits.
    fn bind(
        &mut self,
        root: usize,
        set: TypeSet,
        target: usize,
        kind: TypeSet,
    ) -> Result<(), Mismatch> {
        if set.intersect(kind).is_empty() || self.occurs(root, TyVar(target)) {
            return Err(Mismatch);
        }
        self.slots[root] = Slot::Link(target);
        Ok(())
    }

    /// Is the variable at `root` part of `var`?
    fn occurs(&self, root: usize, var: TyVar) -> bool {
        let found = self.root(var);
        found == root
            || match self.slots[found] {
                Slot::Array(elem) => self.occurs(root, elem),
                Slot::Tuple(t) => self.tuples[t].iter().any(|&e| self.occurs(root, e)),
                _ => false,
            }
    }

    /// Narrows `var` to the types in `allowed`. On failure nothing changes.
    pub fn restrict(&mut self, var: TyVar, allowed: TypeSet) -> Result<(), Mismatch> {
        let narrowed = self.set(var).intersect(allowed);
        if narrowed.is_empty() {
            return Err(Mismatch);
        }

        let root = self.root(var);
        if let Slot::Root(_) = self.slots[root] {
            self.slots[root] = Slot::Root(narrowed);
        }
        Ok(())
    }

    /// Does `var` have a final type, directly or through a default?
    /// `[_]` does not: an empty array says nothing about its elements.
    pub fn is_resolved(&self, var: TyVar) -> bool {
        match self.slots[self.root(var)] {
            Slot::Root(set) => set.default_type().is_some(),
            Slot::Array(elem) => self.is_resolved(elem),
            Slot::Tuple(t) => self.tuples[t].iter().all(|&e| self.is_resolved(e)),
            Slot::Link(_) => unreachable!("root() returns a root"),
        }
    }

    /// For messages: like `TypeSet`'s `Display`, plus arrays and tuples:
    /// "`[real]`", "`(int, real)`".
    pub fn describe(&self, var: TyVar) -> String {
        match self.slots[self.root(var)] {
            Slot::Root(set) => set.to_string(),
            _ => format!("`{}`", self.name(var)),
        }
    }

    /// `[real]`, `[[int]]`. An element with several candidates shows its
    /// default, one with no default shows `_`.
    fn name(&self, var: TyVar) -> String {
        match self.slots[self.root(var)] {
            Slot::Root(set) => set
                .default_type()
                .map_or("_".to_owned(), |ty| type_name(ty).to_owned()),
            Slot::Array(elem) => format!("[{}]", self.name(elem)),
            Slot::Tuple(t) => {
                let names: Vec<String> = self.tuples[t].iter().map(|&e| self.name(e)).collect();
                format!("({})", names.join(", "))
            }
            Slot::Link(_) => unreachable!("root() returns a root"),
        }
    }
}
