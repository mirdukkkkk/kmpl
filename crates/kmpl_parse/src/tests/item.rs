use kmpl_ast::ImportNames;

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
    assert_eq!(
        error("let x = 1;"),
        "expected `const`, `fn` or `struct`, found `let`"
    );
    assert_eq!(error("const N = 1;"), "expected `:`, found `=`");
    assert_eq!(error("fn f(a) { }"), "expected `:`, found `)`");
    assert_eq!(error("fn f() -> { }"), "expected identifier, found `{`");
    assert_eq!(
        error("fn f<T U>() { }"),
        "expected `>`, found identifier `U`"
    );
    assert_eq!(error("fn f<>() { }"), "expected identifier, found `>`");
    assert_eq!(
        error("fn f<real>() { }"),
        "`real` is a built-in type, choose another name for the generic parameter"
    );
    assert_eq!(
        error("fn f<T: text>() { }"),
        "unknown type `text` in bound, use a type or `number`, `float`, `vector`"
    );
    assert_eq!(
        error("fn f<T: int |>() { }"),
        "expected identifier, found `>`"
    );
}

#[test]
fn generic_functions() {
    assert_eq!(
        tree("fn id<T>(x: T) -> T { return x; }"),
        "(fn id<T: int|real|bool|complex|vec2|vec3|vec4|str|bytes> [(x T)] T [(return x)])"
    );
    assert_eq!(
        tree("fn lerp<T: float | vector>(a: T, b: T, t: real) -> T { }"),
        "(fn lerp<T: real|complex|vec2|vec3|vec4> [(a T) (b T) (t real)] T [])"
    );
    assert_eq!(
        tree("fn f<A: int | real, B: vec3,>() { }"),
        "(fn f<A: int|real, B: vec3> [] _ [])"
    );
}

#[test]
fn bounds_merge_duplicates() {
    assert_eq!(
        tree("fn f<T: number | float | int>() { }"),
        "(fn f<T: int|real|complex> [] _ [])"
    );
}

#[test]
fn generic_call_is_an_ordinary_call() {
    // No turbofish: `T` is inferred from the arguments.
    assert_eq!(
        tree("fn main() { lerp(a, b, 0.5); }"),
        "(fn main [] _ [(call lerp [a b 0.5])])"
    );
}

#[test]
fn imports() {
    let program =
        parse_program(r#"import fs from "std:fs"; import (a, b as c,) from "./x"; fn main() { }"#)
            .unwrap();

    let [first, second] = &program.imports[..] else {
        panic!("expected two imports");
    };
    assert_eq!(first.source, "std:fs");
    assert!(matches!(&first.names, ImportNames::Module(name) if name.name == "fs"));

    assert_eq!(second.source, "./x");
    let ImportNames::Items(items) = &second.names else {
        panic!("expected a list of names");
    };
    let names: Vec<(&str, &str)> = items
        .iter()
        .map(|i| (i.name.name.as_str(), i.alias.name.as_str()))
        .collect();
    assert_eq!(names, [("a", "a"), ("b", "c")]);
}

#[test]
fn from_and_as_are_ordinary_names() {
    super::body("let from = 1; let as = from;");
}

#[test]
fn import_errors() {
    let error = |src: &str| parse_program(src).unwrap_err().message;
    assert_eq!(error("import a"), "expected `from`, found end of file");
    assert_eq!(error("import a.b;"), "expected `from`, found `.`");
    assert_eq!(error("import ;"), "expected identifier, found `;`");
    assert_eq!(
        error("import a from b;"),
        "expected a string, found identifier `b`"
    );
    assert_eq!(
        error(r#"import a from "x""#),
        "expected `;`, found end of file"
    );
    assert_eq!(
        error(r#"import () from "x";"#),
        "expected identifier, found `)`"
    );
    assert_eq!(
        error(r#"fn f() { } import a from "x";"#),
        "expected `const`, `fn` or `struct`, found `import`"
    );
}

#[test]
fn structs() {
    let program = parse_program("struct P { x: real, y: [int], q: m.Q, } struct E { }").unwrap();
    assert_eq!(
        sexp_program(&program),
        "(struct P [(x real) (y [int]) (q m.Q)]) (struct E [])"
    );
    let error = |src: &str| parse_program(src).unwrap_err().message;
    assert_eq!(error("struct { }"), "expected identifier, found `{`");
    assert_eq!(error("struct P { x }"), "expected `:`, found `}`");
    assert_eq!(
        error("struct P { x: real y: int }"),
        "expected `}`, found identifier `y`"
    );
}
