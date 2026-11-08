use kmpl_ast::Type;

use crate::types::{TypeSet, Unifier};

#[test]
fn sets() {
    assert!(TypeSet::NUMBER.contains(Type::Real));
    assert!(!TypeSet::NUMBER.contains(Type::Bool));
    assert!(TypeSet::FLOAT.is_subset(TypeSet::NUMBER));
    assert!(!TypeSet::NUMBER.is_subset(TypeSet::FLOAT));
    assert_eq!(TypeSet::NUMBER.intersect(TypeSet::FLOAT), TypeSet::FLOAT);
    assert!(TypeSet::ORDERED.intersect(TypeSet::VECTOR).is_empty());
    assert_eq!(TypeSet::single(Type::Vec3).as_single(), Some(Type::Vec3));
    assert_eq!(TypeSet::FLOAT.as_single(), None);
}

#[test]
fn defaults() {
    assert_eq!(TypeSet::NUMBER.default_type(), Some(Type::Int));
    assert_eq!(TypeSet::FLOAT.default_type(), Some(Type::Real));
    assert_eq!(
        TypeSet::single(Type::Complex).default_type(),
        Some(Type::Complex)
    );
    assert_eq!(TypeSet::single(Type::Bool).default_type(), Some(Type::Bool));
    assert_eq!(TypeSet::VECTOR.default_type(), None);
    assert_eq!(TypeSet::ANY.default_type(), None);
}

#[test]
fn display() {
    assert_eq!(TypeSet::single(Type::Real).to_string(), "`real`");
    assert_eq!(TypeSet::FLOAT.to_string(), "`real` or `complex`");
    assert_eq!(TypeSet::NUMBER.to_string(), "`int`, `real` or `complex`");
    assert_eq!(TypeSet::ANY.to_string(), "any type");
}

#[test]
fn unify_intersects() {
    let mut u = Unifier::default();
    let int_lit = u.new_var(TypeSet::NUMBER);
    let real_lit = u.new_var(TypeSet::FLOAT);

    u.unify(int_lit, real_lit).unwrap();
    assert_eq!(u.set(int_lit), TypeSet::FLOAT);
    assert_eq!(u.set(real_lit), TypeSet::FLOAT);
}

#[test]
fn unify_is_transitive() {
    let mut u = Unifier::default();
    let a = u.new_var(TypeSet::ANY);
    let b = u.new_var(TypeSet::NUMBER);
    let c = u.new_var(TypeSet::single(Type::Real));

    u.unify(a, b).unwrap();
    u.unify(b, c).unwrap();
    assert_eq!(u.set(a), TypeSet::single(Type::Real));
}

#[test]
fn unify_mismatch_changes_nothing() {
    let mut u = Unifier::default();
    let real = u.new_var(TypeSet::single(Type::Real));
    let boolean = u.new_var(TypeSet::single(Type::Bool));

    assert_eq!(
        u.unify(real, boolean),
        Err((TypeSet::single(Type::Real), TypeSet::single(Type::Bool)))
    );
    assert_eq!(u.set(real), TypeSet::single(Type::Real));
    assert_eq!(u.set(boolean), TypeSet::single(Type::Bool));
}

#[test]
fn unify_with_itself() {
    let mut u = Unifier::default();
    let a = u.new_var(TypeSet::NUMBER);
    u.unify(a, a).unwrap();
    assert_eq!(u.set(a), TypeSet::NUMBER);
}

#[test]
fn restrict() {
    let mut u = Unifier::default();
    let a = u.new_var(TypeSet::NUMBER);
    let b = u.new_var(TypeSet::ANY);
    u.unify(a, b).unwrap();

    u.restrict(b, TypeSet::ORDERED).unwrap();
    assert_eq!(u.set(a), TypeSet::of(&[Type::Int, Type::Real]));

    assert_eq!(
        u.restrict(a, TypeSet::VECTOR),
        Err(TypeSet::of(&[Type::Int, Type::Real]))
    );
    assert_eq!(u.set(a), TypeSet::of(&[Type::Int, Type::Real]));
}
