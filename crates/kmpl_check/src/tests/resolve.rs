use super::{assert_error, assert_ok, errors};

#[test]
fn enum_names_are_checked() {
    assert_error("enum A { X, X }", "variant `X` is already defined in `A`");
    assert_error("enum A { X(B) }", "unknown type `B`");
    assert_error("enum A { X } const A: int = 1;", "`A` is already defined");
    assert_error("enum A { X } fn f() { let a = B::X; }", "unknown enum `B`");
    assert_error(
        "enum A { X } fn f() { let a = A::Y; }",
        "`A` has no variant `Y`",
    );
    assert_error(
        "struct A { } fn f() { let a = A::Y; }",
        "`A` is not an enum",
    );
    assert_error(
        "enum A { X(int) } fn f() { let a = A::X; }",
        "`A::X` holds 1 values, but 0 were given",
    );
    assert_error(
        "enum A { X(int) } fn f(a: A) { match a { A::X(b, c) => { } } }",
        "`A::X` holds 1 values, but the pattern names 2",
    );
    assert_error(
        "enum A { X } fn f() { let a = A; }",
        "`A` is an enum, not a value",
    );
    assert_ok("enum Tree { Leaf, Node(Tree, Tree) }");
    assert_ok("struct Node { next: Node? }");
}

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

#[test]
fn constants_call_functions_when_it_is_safe() {
    assert_ok("const A: real = sqrt(2.0);");
    assert_ok("const A: real = 1.0; const B: real = max(A, 2.0);");
    assert_ok("fn f() -> real { return A; } const A: real = 1.0;");
    assert_ok("fn f() -> real { return 1.0; } const A: real = f();");
    assert_ok("fn f() -> real { return 1.0; } const A: real = sqrt(f());");
    assert_ok(
        "const A: int = 2; fn twice(n: int) -> int { return n * A; } const B: int = twice(3);",
    );
    // Through other functions, in any order.
    assert_ok(
        "const A: int = 2; const B: int = f(); fn f() -> int { return g(); } fn g() -> int { return A; }",
    );
    // Recursion ends the search.
    assert_ok(
        "fn f(n: int) -> int { if n == 0 { return 0; } return f(n - 1); } const A: int = f(3);",
    );
}

#[test]
fn a_constant_cannot_reach_a_constant_without_a_value() {
    assert_error(
        "fn f() -> real { return B; } const A: real = f(); const B: real = 2.0;",
        "a constant cannot call `f`: it reads `B`, which has no value yet",
    );
    assert_error(
        "fn f() -> real { return A; } const A: real = f();",
        "a constant cannot call `f`: it reads `A`, which has no value yet",
    );
    assert_error(
        "fn g() -> int { return B; } fn f() -> int { return g(); } const A: int = f(); const B: int = 1;",
        "a constant cannot call `f`: it reads `B`, which has no value yet",
    );
}

#[test]
fn a_local_variable_is_not_the_constant_of_that_name() {
    assert_ok("fn f(B: int) -> int { return B; } const A: int = f(1); const B: int = 2;");
    assert_ok("fn f() -> int { let B = 1; return B; } const A: int = f(); const B: int = 2;");
    assert_ok(
        "fn f() -> int { for B in 0..1 { return B; } return 0; } const A: int = f(); const B: int = 2;",
    );
}

#[test]
fn a_function_that_depends_on_the_run_cannot_give_a_constant() {
    assert_error(
        "fn f() -> int { return length(_io_args()); } const A: int = f();",
        "a constant cannot call `f`: its value depends on how the program is run",
    );
}

#[test]
fn compound_assignment_targets() {
    assert_error(
        "const A: int = 1; fn f() { A += 1; }",
        "cannot assign to constant `A`",
    );
    assert_error(
        "fn f() { for i in 0..1 { i += 1; } }",
        "cannot assign to loop variable `i`",
    );
    assert_error("fn f() { x += 1; }", "undefined variable `x`");
    assert_error("fn f(a: [int]) { a[y] += 1; }", "undefined variable `y`");
}

#[test]
fn step_and_slices_resolve_their_names() {
    assert_error(
        "fn f() { for i in 0..1 step s { } }",
        "undefined variable `s`",
    );
    assert_error(
        "fn f() { for i in 0..1 step i { } }",
        "undefined variable `i`",
    );
    assert_error("fn f(a: [int]) { a[n..]; }", "undefined variable `n`");
    assert_error("fn f(a: [int]) { a[..n]; }", "undefined variable `n`");
}

#[test]
fn unpacking_declares_names() {
    assert_ok("fn f(t: (int, int)) { let (a, b) = t; print(a + b); }");
    assert_ok("fn f(t: (int, int)) { let (_, _) = t; }");
    assert_error(
        "fn f(t: (int, int)) { let (a, a) = t; }",
        "`a` is already defined in this scope",
    );
    assert_error(
        "fn f(t: (int, int)) { let (a, _) = t; print(_); }",
        "undefined variable `_`",
    );
    // The value is checked first, like `let x = x;`.
    assert_error("fn f() { let (a, b) = (a, 1); }", "undefined variable `a`");
    assert_error("fn f(t: (int, Foo)) { }", "unknown type `Foo`");
}

#[test]
fn struct_definitions() {
    assert_ok("struct P { x: real } struct L { a: P, b: [P], c: (P, int) } struct Empty { }");
    assert_ok("fn f(p: P) -> P { return p; } struct P { x: real }");
    assert_error(
        "struct P { x: real, x: int }",
        "field `x` is already defined in `P`",
    );
    assert_error("struct P { x: Q }", "unknown type `Q`");
    assert_error("struct P { } struct P { }", "`P` is already defined");
    assert_error("struct P { } fn P() { }", "`P` is already defined");
    assert_error("struct sqrt { }", "`sqrt` is a built-in function");
    assert_error("struct bool { }", "`bool` is a built-in type");
}

#[test]
fn structs_cannot_contain_themselves() {
    let message = "`A` contains itself, so it would be infinitely large; use an array of `A`";
    assert_error("struct A { a: A }", message);
    assert_error("struct A { t: (int, A) }", message);
    assert_eq!(
        errors("struct A { b: B } struct B { a: A }"),
        [
            message,
            "`B` contains itself, so it would be infinitely large; use an array of `B`"
        ]
    );
    assert_ok("struct A { children: [A] }");
    assert_ok("struct A { b: [B] } struct B { a: A }");
    // Reaching a cycle is not being part of it: reported for the cycle only.
    assert_eq!(errors("struct C { a: A } struct A { a: A }"), [message]);
}

#[test]
fn struct_literal_fields() {
    let p = "struct P { x: real, y: real }";
    assert_ok(&format!("{p} fn f() {{ P {{ y: 1, x: 2 }}; }}"));
    assert_error(
        &format!("{p} fn f() {{ P {{ x: 1, y: 2, z: 3 }}; }}"),
        "`P` has no field `z`",
    );
    assert_error(
        &format!("{p} fn f() {{ P {{ x: 1, x: 2, y: 3 }}; }}"),
        "field `x` is specified twice",
    );
    assert_error(
        &format!("{p} fn f() {{ P {{ x: 1 }}; }}"),
        "missing field `y` in `P`",
    );
    assert_error(
        &format!("{p} fn f() {{ P {{ }}; }}"),
        "missing fields `x`, `y` in `P`",
    );
    assert_error("fn f() { Q { }; }", "unknown struct `Q`");
    assert_error("fn g() { } fn f() { g { }; }", "`g` is not a struct");
    assert_error("fn f() { sqrt { }; }", "`sqrt` is not a struct");
    assert_error(
        "fn f() { P { x: y }; } struct P { x: int }",
        "undefined variable `y`",
    );
}

#[test]
fn struct_names_are_not_values() {
    assert_error(
        "struct P { } fn f() { print(P); }",
        "`P` is a struct, not a value",
    );
    assert_error(
        "struct P { } fn f() { P = 1; }",
        "cannot assign to struct `P`",
    );
    assert_error("struct P { } fn f() { P(); }", "`P` is not a function");
    assert_error(
        "struct P { x: int } fn f(p: P) { p.x = y; }",
        "undefined variable `y`",
    );
    assert_error(
        "struct P { x: [int] } const C: P = P { x: [1] }; fn f() { C.x[0] = 2; }",
        "cannot assign to constant `C`",
    );
    assert_error(
        "struct P { x: int } fn f(a: [P]) { for p in a { p.x = 1; } }",
        "cannot assign to loop variable `p`",
    );
}

#[test]
fn constants_cannot_depend_on_the_run() {
    assert_error(
        "const A: [string] = _io_args();",
        "a constant cannot call `_io_args`: its value depends on how the program is run",
    );
    assert_error(
        "const A: int = length(_io_args());",
        "a constant cannot call `_io_args`: its value depends on how the program is run",
    );
    assert_error(
        "const A: string? = _io_read_line();",
        "a constant cannot call `_io_read_line`: its value depends on how the program is run",
    );
    assert_ok(r#"const A: int? = _text_parse_int("1");"#);
    assert_error(
        "fn f() { _io_args(1); }",
        "`_io_args` takes 0 arguments, but 1 was given",
    );
    assert_error("fn length() { }", "`length` is a built-in function");
    assert_ok("fn f() { let args = 1; print(args); }");
}
