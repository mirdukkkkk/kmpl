use crate::check;

/// Error messages for `src`, in the order they were found.
fn errors(src: &str) -> Vec<String> {
    let tokens = kmpl_lexer::lex(src).unwrap();
    let program = kmpl_parse::parse(tokens).unwrap();
    match check(&program) {
        Ok(()) => Vec::new(),
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
    }
}

fn assert_ok(src: &str) {
    assert_eq!(errors(src), Vec::<String>::new(), "source: {src}");
}

/// `src` must give exactly one error, with this message.
fn assert_error(src: &str, message: &str) {
    assert_eq!(errors(src), vec![message.to_owned()], "source: {src}");
}

#[test]
fn valid_program() {
    assert_ok(
        "const N: int = 10;
         const M: int = N * 2;
         fn main() {
             let x = sqrt(2.0);
             let v = vec3(x, x, x);
             x = helper(N, M);
             print(v:zyx);
         }
         fn helper(a: int, b: int) -> int { return a + b; }",
    );
}

#[test]
fn undefined_names() {
    assert_error("fn main() { print(x); }", "undefined variable `x`");
    assert_error("fn main() { foo(); }", "undefined function `foo`");
    assert_error("fn main() { x = 1; }", "undefined variable `x`");
}

#[test]
fn duplicate_globals() {
    assert_error("fn f() { } fn f() { }", "`f` is already defined");
    assert_error("const A: int = 1; fn A() { }", "`A` is already defined");
    assert_error("fn sqrt(x: real) { }", "`sqrt` is a built-in function");
    assert_error("const print: int = 1;", "`print` is a built-in function");
}

#[test]
fn duplicate_locals() {
    assert_error(
        "fn f(a: int, a: real) { }",
        "`a` is already defined in this scope",
    );
    assert_error(
        "fn f() { let x = 1; let x = 2; }",
        "`x` is already defined in this scope",
    );
}

#[test]
fn shadowing_in_inner_block_is_allowed() {
    assert_ok("fn f(a: int) { let a = 1; while true { let a = 2.0; } }");
    assert_ok("const N: int = 1; fn f() { let N = 2; }");
}

#[test]
fn let_initializer_sees_outer_name() {
    assert_ok("fn f(x: int) { let x = x + 1; }");
    assert_error("fn f() { let y = y; }", "undefined variable `y`");
}

#[test]
fn locals_do_not_leak_out_of_blocks() {
    assert_error(
        "fn f() { if true { let t = 1; } print(t); }",
        "undefined variable `t`",
    );
    assert_error(
        "fn f() { while true { let t = 1; } t = 2; }",
        "undefined variable `t`",
    );
    assert_error(
        "fn f() { let a = 1; } fn g() { print(a); }",
        "undefined variable `a`",
    );
}

#[test]
fn assign_to_non_variable() {
    assert_error(
        "const N: int = 1; fn f() { N = 2; }",
        "cannot assign to constant `N`",
    );
    assert_error("fn f() { f = 2; }", "cannot assign to function `f`");
    assert_error("fn f() { sqrt = 2; }", "cannot assign to function `sqrt`");
}

#[test]
fn function_used_as_value_and_back() {
    assert_error(
        "fn f() { let x = f; }",
        "`f` is a function, call it with `f(...)`",
    );
    assert_error("fn f() { let x = 1; x(); }", "`x` is not a function");
    assert_error("const N: int = 1; fn f() { N(); }", "`N` is not a function");
}

#[test]
fn functions_can_be_called_before_definition() {
    assert_ok("fn main() { later(); } fn later() { }");
}

#[test]
fn constants_only_see_earlier_constants() {
    assert_error(
        "const A: int = B; const B: int = 1;",
        "constant `B` is used before its definition",
    );
    assert_error(
        "const A: int = A;",
        "constant `A` is used before its definition",
    );
    assert_ok("fn f() { print(LATER); } const LATER: int = 1;");
}

#[test]
fn reports_all_errors() {
    assert_eq!(
        errors("fn f() { a = 1; b(); print(c); }"),
        vec![
            "undefined variable `a`",
            "undefined function `b`",
            "undefined variable `c`",
        ]
    );
}
