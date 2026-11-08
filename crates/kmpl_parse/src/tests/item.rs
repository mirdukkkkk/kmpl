use kmpl_ast::{Item, Span};

use super::{parse_program, sexp_program};

fn tree(src: &str) -> String {
    sexp_program(&parse_program(src).unwrap())
}

fn error(src: &str) -> String {
    parse_program(src).unwrap_err().message
}

#[test]
fn empty_program() {
    assert_eq!(tree(""), "");
}

#[test]
fn const_item() {
    assert_eq!(tree("const N: int = 10;"), "(const N int 10)");
    assert_eq!(tree("const I: complex = 1.0i;"), "(const I complex 1.0i)");
}

#[test]
fn functions() {
    assert_eq!(tree("fn main() { }"), "(fn main [] _ [])");
    assert_eq!(
        tree("fn f(a: real, b: int) -> real { return a; }"),
        "(fn f [(a real) (b int)] real [(return a)])"
    );
    assert_eq!(tree("fn g(v: vec3,) { }"), "(fn g [(v vec3)] _ [])");
}

#[test]
fn several_items() {
    assert_eq!(
        tree("const K: real = 2.0; fn main() { print(K); }"),
        "(const K real 2.0) (fn main [] _ [(call print [K])])"
    );
}

#[test]
fn function_span() {
    let program = parse_program("fn f() { }  ").unwrap();
    let Item::Function(f) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(f.span, Span::new(0, 10));
}

#[test]
fn errors() {
    assert_eq!(error("let x = 1;"), "expected `const` or `fn`, found `let`");
    assert_eq!(error("const N = 1;"), "expected `:`, found `=`");
    assert_eq!(error("fn f(a) { }"), "expected `:`, found `)`");
    assert_eq!(error("fn f() -> { }"), "expected identifier, found `{`");
    assert_eq!(error("fn f() -> str { }"), "unknown type `str`");
}
