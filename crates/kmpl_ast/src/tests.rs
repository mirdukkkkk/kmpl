use crate::{Ident, Span};

#[test]
fn span_new() {
    let s = Span::new(3, 7);
    assert_eq!((s.start, s.end), (3, 7));
}

#[test]
fn span_from_range() {
    assert_eq!(Span::from(2..5), Span::new(2, 5));
    assert_eq!(Span::from(0..0), Span::new(0, 0));
}

#[test]
fn span_to_covers_both() {
    let a = Span::new(0, 3);
    let b = Span::new(6, 10);
    assert_eq!(a.to(b), Span::new(0, 10));
}

#[test]
fn span_to_itself() {
    let a = Span::new(4, 8);
    assert_eq!(a.to(a), a);
}

#[test]
fn span_is_copy() {
    let a = Span::new(1, 2);
    let b = a;
    assert_eq!(a, b);
}

#[test]
fn ident_equality_includes_span() {
    let a = Ident {
        name: "x".into(),
        span: Span::new(0, 1),
    };
    let b = Ident {
        name: "x".into(),
        span: Span::new(5, 6),
    };
    assert_ne!(a, b);
    assert_eq!(a, a.clone());
}
