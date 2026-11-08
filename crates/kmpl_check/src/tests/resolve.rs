use super::{assert_error, assert_ok, errors};

#[test]
fn valid_program() {
    assert_ok(
        "const N: int = 10;
         const M: int = N * 2;
         fn main() {
             let x = sqrt(2.0);
             let v = vec3(x, x, x);
             let n = helper(N, M);
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
fn builtin_constants() {
    assert_ok("fn f() { let x = PI * 2; let y = E + TAU; }");
    assert_ok("const HALF_PI: real = PI / 2; fn f() { }");
    assert_ok("fn f() { let PI = 3; }");
    assert_error("const PI: real = 3.0;", "`PI` is a built-in constant");
    assert_error("fn E() { }", "`E` is a built-in constant");
    assert_error("fn f() { PI = 3.0; }", "cannot assign to constant `PI`");
    assert_error("fn f() { PI(); }", "`PI` is not a function");
}

#[test]
fn for_variable_scope() {
    assert_ok("fn f() { for i in 0..3 { print(i); } }");
    assert_error(
        "fn f() { for i in 0..3 { } print(i); }",
        "undefined variable `i`",
    );
    // The bounds do not see the loop variable.
    assert_error("fn f() { for i in 0..i { } }", "undefined variable `i`");
    assert_ok("fn f() { let i = 3; for i in 0..i { } }");
    // Nested and consecutive loops may reuse a name.
    assert_ok("fn f() { for i in 0..3 { for i in 0..i { } } for i in 0..3 { } }");
}

#[test]
fn for_variable_cannot_be_assigned() {
    assert_error(
        "fn f() { for i in 0..3 { i = 1; } }",
        "cannot assign to loop variable `i`",
    );
    assert_error(
        "fn f() { for i in 0..3 { if true { i = 1; } } }",
        "cannot assign to loop variable `i`",
    );
}

#[test]
fn for_variable_can_be_shadowed() {
    assert_ok("fn f() { for i in 0..3 { let i = 5; i = 6; print(i); } }");
    // An outer variable with the same name stays assignable after the loop.
    assert_ok("fn f() { let i = 1; for i in 0..3 { } i = 2; print(i); }");
}

#[test]
fn break_and_continue_inside_for() {
    assert_ok("fn f() { for i in 0..3 { if i == 1 { continue; } break; } }");
    assert_error(
        "fn f() { for i in 0..3 { } break; }",
        "`break` outside of a loop",
    );
}

#[test]
fn names_inside_arrays() {
    assert_error("fn f() { [x]; }", "undefined variable `x`");
    assert_error("fn f() { [0; x]; }", "undefined variable `x`");
    assert_error("fn f(a: [int]) { a[x]; }", "undefined variable `x`");
    assert_error("fn f() { y[0] = 1; }", "undefined variable `y`");
    assert_error("fn f(a: [int]) { a[x] = 1; }", "undefined variable `x`");
    assert_error("fn f(a: [int]) { a[0] = x; }", "undefined variable `x`");
}

#[test]
fn index_assignment_targets() {
    assert_error(
        "const A: [int] = [1]; fn f() { A[0] = 2; }",
        "cannot assign to constant `A`",
    );
    assert_error(
        "fn f() { for x in [[1]] { x[0] = 2; } }",
        "cannot assign to loop variable `x`",
    );
}

#[test]
fn array_type_names_are_checked() {
    assert_error("fn f() { let a: [Foo]; }", "unknown type `Foo`");
    assert_error("fn f(a: [[Foo]]) { }", "unknown type `Foo`");
    assert_ok("fn f<T: int | real>(a: [T]) { }");
}

#[test]
fn for_each_scope() {
    assert_ok("fn f(a: [int]) { for x in a { print(x); } }");
    assert_error(
        "fn f(a: [int]) { for x in a { } print(x); }",
        "undefined variable `x`",
    );
    assert_error(
        "fn f(a: [int]) { for x in a { x = 1; } }",
        "cannot assign to loop variable `x`",
    );
    assert_error("fn f() { for x in x { } }", "undefined variable `x`");
    assert_ok("fn f(a: [int]) { for x in a { if x == 1 { continue; } break; } }");
}
