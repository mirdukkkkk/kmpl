use crate::check;

/// Error messages for `src`, in the order they were found.
/// Adds an empty `fn main() { }` if `src` has no `main`, so tests
/// about other rules do not also get "no `main`".
fn errors(src: &str) -> Vec<String> {
    let src = if src.contains("main") {
        src.to_owned()
    } else {
        format!("{src} fn main() {{ }}")
    };
    errors_exact(&src)
}

/// Error messages for `src` exactly as written.
fn errors_exact(src: &str) -> Vec<String> {
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

#[test]
fn break_and_continue_need_a_loop() {
    assert_error("fn f() { break; }", "`break` outside of a loop");
    assert_error(
        "fn f() { if true { continue; } }",
        "`continue` outside of a loop",
    );
    assert_ok("fn f() { while true { if true { break; } continue; } }");
    assert_ok("fn f() { while true { while false { break; } break; } }");
}

#[test]
fn loop_depth_resets_after_loop() {
    assert_error(
        "fn f() { while true { } break; }",
        "`break` outside of a loop",
    );
}

#[test]
fn argument_counts() {
    assert_error(
        "fn g(a: int) { } fn f() { g(); }",
        "`g` takes 1 argument, but 0 were given",
    );
    assert_error(
        "fn g() { } fn f() { g(1); }",
        "`g` takes 0 arguments, but 1 was given",
    );
    assert_error(
        "fn f() { let v = vec3(1.0, 2.0); }",
        "`vec3` takes 3 arguments, but 2 were given",
    );
    assert_error(
        "fn f() { print(1, 2); }",
        "`print` takes 1 argument, but 2 were given",
    );
    assert_ok("fn g(a: int, b: real) { } fn f() { g(1, 2.0); }");
}

#[test]
fn main_is_required() {
    assert_eq!(
        errors_exact("fn f() { }"),
        vec!["no `main` function in the program"]
    );
    assert_eq!(errors_exact(""), vec!["no `main` function in the program"]);
    assert_eq!(
        errors_exact("const main: int = 1;"),
        vec!["`main` must be a function"]
    );
}

#[test]
fn main_signature() {
    assert_error("fn main(a: int) { }", "`main` must not take parameters");
    assert_error(
        "fn main() -> int { return 0; }",
        "`main` must not return a value",
    );
    assert_eq!(
        errors_exact("fn main(a: int) -> int { return a; }"),
        vec![
            "`main` must not take parameters",
            "`main` must not return a value",
        ]
    );
}

const UNINIT_X: &str = "`x` is used before being assigned";

#[test]
fn read_before_assign() {
    assert_error("fn f() { let x: int; print(x); }", UNINIT_X);
    assert_error("fn f() { let x; let y = x + 1; }", UNINIT_X);
    assert_ok("fn f() { let x: int; x = 1; print(x); }");
    assert_ok("fn f(x: int) { print(x); }");
}

#[test]
fn assign_reads_value_first() {
    assert_error("fn f() { let x; x = x + 1; }", UNINIT_X);
}

#[test]
fn if_else_must_assign_in_both_branches() {
    assert_ok("fn f(c: bool) { let x; if c { x = 1; } else { x = 2; } print(x); }");
    assert_error(
        "fn f(c: bool) { let x; if c { x = 1; } else { } print(x); }",
        UNINIT_X,
    );
    assert_error(
        "fn f(c: bool) { let x; if c { x = 1; } print(x); }",
        UNINIT_X,
    );
}

#[test]
fn else_if_chain() {
    assert_ok(
        "fn f(a: bool, b: bool) {
             let x;
             if a { x = 1; } else if b { x = 2; } else { x = 3; }
             print(x);
         }",
    );
    assert_error(
        "fn f(a: bool, b: bool) {
             let x;
             if a { x = 1; } else if b { x = 2; }
             print(x);
         }",
        UNINIT_X,
    );
}

#[test]
fn while_body_may_not_run() {
    assert_error(
        "fn f(c: bool) { let x; while c { x = 1; } print(x); }",
        UNINIT_X,
    );
    assert_ok("fn f(c: bool) { let x; while c { x = 1; print(x); } }");
}

#[test]
fn diverging_branch_does_not_block() {
    assert_ok("fn f(c: bool) { let x; if c { x = 1; } else { return; } print(x); }");
    assert_ok("fn f(c: bool) { let x; if c { return; } else { x = 1; } print(x); }");
    assert_ok("fn f(c: bool) { while true { let x; if c { x = 1; } else { break; } print(x); } }");
    assert_ok(
        "fn f(c: bool) { while true { let x; if c { continue; } else { x = 1; } print(x); } }",
    );
}

#[test]
fn code_after_return_reports_nothing() {
    assert_ok("fn f() { let x; return; print(x); }");
}

#[test]
fn shadowing_tracks_each_variable() {
    assert_error(
        "fn f() { let x; while true { let x = 1; print(x); break; } print(x); }",
        UNINIT_X,
    );
    assert_error(
        "fn f(c: bool) { let x; if c { let x = 1; x = 2; } else { } print(x); }",
        UNINIT_X,
    );
    assert_error(
        "fn f() { let x; while true { let x = x; break; } }",
        UNINIT_X,
    );
}

#[test]
fn one_error_per_variable_on_a_path() {
    assert_error("fn f() { let x; print(x); print(x); }", UNINIT_X);
}
