use kmpl_ast::Span;

use super::{parse_expr as parse, sexp};

fn tree(src: &str) -> String {
    sexp(&parse(src).unwrap())
}

fn error(src: &str) -> String {
    parse(src).unwrap_err().message
}

#[test]
fn literals() {
    assert_eq!(tree("42"), "42");
    assert_eq!(tree("1.5"), "1.5");
    assert_eq!(tree("2.0i"), "2.0i");
    assert_eq!(tree("true"), "true");
    assert_eq!(tree("x"), "x");
}

#[test]
fn mul_binds_tighter_than_add() {
    assert_eq!(tree("1 + 2 * 3"), "(+ 1 (* 2 3))");
    assert_eq!(tree("1 * 2 + 3"), "(+ (* 1 2) 3)");
}

#[test]
fn left_associative() {
    assert_eq!(tree("a - b - c"), "(- (- a b) c)");
    assert_eq!(tree("a / b / c"), "(/ (/ a b) c)");
}

#[test]
fn parens_override_precedence() {
    assert_eq!(tree("(1 + 2) * 3"), "(* (+ 1 2) 3)");
}

#[test]
fn full_precedence_chain() {
    assert_eq!(
        tree("a || b && c == d < e + f * -g"),
        "(|| a (&& b (== c (< d (+ e (* f (- g)))))))"
    );
}

#[test]
fn unary() {
    assert_eq!(tree("-x"), "(- x)");
    assert_eq!(tree("!!b"), "(! (! b))");
    assert_eq!(tree("-a * b"), "(* (- a) b)");
}

#[test]
fn swizzle_binds_tighter_than_unary() {
    assert_eq!(tree("v:zyx"), "(: v zyx)");
    assert_eq!(tree("-v:xy"), "(- (: v xy))");
    assert_eq!(tree("f(v):x + 1"), "(+ (: (call f [v]) x) 1)");
}

#[test]
fn calls() {
    assert_eq!(tree("f()"), "(call f [])");
    assert_eq!(tree("vec3(1.0, 2.0, 3.0)"), "(call vec3 [1.0 2.0 3.0])");
    assert_eq!(tree("f(a,)"), "(call f [a])");
    assert_eq!(tree("f(g(x), y + 1)"), "(call f [(call g [x]) (+ y 1)])");
}

#[test]
fn spans() {
    let e = parse("a + f(b)").unwrap();
    assert_eq!(e.span, Span::new(0, 8));

    let e = parse("-v:xy").unwrap();
    assert_eq!(e.span, Span::new(0, 5));
}

#[test]
fn errors() {
    assert_eq!(error("1 +"), "expected expression, found end of file");
    assert_eq!(error("(1 + 2"), "expected `)`, found end of file");
    assert_eq!(error("f(1 2)"), "expected `)`, found number `2`");
    assert_eq!(error("v:1"), "expected identifier, found number `1`");
    assert_eq!(error("1 2"), "expected end of expression, found number `2`");
}

#[test]
fn error_span_points_at_bad_token() {
    let err = parse("1 + ;").unwrap_err();
    assert_eq!(err.span, Span::new(4, 5));
}

#[test]
fn matrix_cells() {
    assert_eq!(tree("m[i, j]"), "(cell m i j)");
    assert_eq!(tree("m[i + 1, 2 * j]"), "(cell m (+ i 1) (* 2 j))");
    assert_eq!(tree("a.m[i, j]"), "(cell (. a m) i j)");
    assert_eq!(tree("a[k][i, j]"), "(cell (index a k) i j)");
    assert_eq!(tree("m[i, j] * 2"), "(* (cell m i j) 2)");
}

#[test]
fn matrix_cell_needs_exactly_two_indices() {
    assert!(error("m[i, j, k]").contains("expected `]`"));
    assert!(error("m[i,]").contains("expected"));
}
