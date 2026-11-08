use kmpl_ast::{Item, Span};

use super::{body, body_error, parse_program};

#[test]
fn unknown_type_names_parse_as_params() {
    // Could be a generic parameter; the checker reports unknown ones.
    assert_eq!(body("let x: T;"), "[(let x T _)]");
}

#[test]
fn let_forms() {
    assert_eq!(body("let x: real = 1.0;"), "[(let x real 1.0)]");
    assert_eq!(body("let x = 1;"), "[(let x _ 1)]");
    assert_eq!(body("let x: vec3;"), "[(let x vec3 _)]");
    assert_eq!(body("let x;"), "[(let x _ _)]");
}

#[test]
fn assign_vs_expr_stmt() {
    assert_eq!(body("x = x + 1;"), "[(= x (+ x 1))]");
    assert_eq!(body("x + 1;"), "[(+ x 1)]");
    assert_eq!(body("print(x);"), "[(call print [x])]");
    assert_eq!(body("x == 1;"), "[(== x 1)]");
}

#[test]
fn if_else() {
    assert_eq!(body("if a { b; }"), "[(if a [b])]");
    assert_eq!(body("if a { b; } else { c; }"), "[(if a [b] [c])]");
}

#[test]
fn else_if_is_nested_if() {
    assert_eq!(
        body("if a { x; } else if b { y; } else { z; }"),
        "[(if a [x] [(if b [y] [z])])]"
    );
}

#[test]
fn while_break_continue() {
    assert_eq!(
        body("while i < 10 { if i == 5 { break; } continue; }"),
        "[(while (< i 10) [(if (== i 5) [break]) continue])]"
    );
}

#[test]
fn return_forms() {
    assert_eq!(body("return;"), "[(return)]");
    assert_eq!(body("return a * 2;"), "[(return (* a 2))]");
}

#[test]
fn empty_and_nested_blocks() {
    assert_eq!(body(""), "[]");
    assert_eq!(
        body("while true { while false { } }"),
        "[(while true [(while false [])])]"
    );
}

#[test]
fn stmt_spans() {
    let program = parse_program("fn f() { let x = 1; if x { } else { } }").unwrap();
    let Item::Function(f) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(f.body[0].span, Span::new(9, 19));
    assert_eq!(f.body[1].span, Span::new(20, 37));
}

#[test]
fn errors() {
    assert_eq!(body_error("let x = 1"), "expected `;`, found `}`");
    assert_eq!(
        body_error("let 1 = x;"),
        "expected identifier, found number `1`"
    );
    assert_eq!(body_error("if a b;"), "expected `{`, found identifier `b`");
    assert_eq!(body_error("break"), "expected `;`, found `}`");
    assert_eq!(body_error("x = ;"), "expected expression, found `;`");
}

#[test]
fn unclosed_block() {
    let err = parse_program("fn f() { let x = 1;").unwrap_err();
    assert_eq!(err.message, "expected `}`, found end of file");
    assert_eq!(err.span, Span::new(19, 19));
}

#[test]
fn for_loop() {
    assert_eq!(body("for i in 0..n { x; }"), "[(for i 0 n [x])]");
    assert_eq!(body("for i in 0..n { }"), "[(for i 0 n [])]");
}

#[test]
fn for_bounds_are_full_expressions() {
    assert_eq!(
        body("for i in a + 1..f(b) * 2 { }"),
        "[(for i (+ a 1) (* (call f [b]) 2) [])]"
    );
}

#[test]
fn for_errors() {
    assert_eq!(
        body_error("for 1 in 0..3 { }"),
        "expected identifier, found number `1`"
    );
    assert_eq!(
        body_error("for i 0..3 { }"),
        "expected `in`, found number `0`"
    );
    assert_eq!(body_error("for i in 0 { }"), "expected `..`, found `{`");
    assert_eq!(
        body_error("for i in 0..3 x;"),
        "expected `{`, found identifier `x`"
    );
}
