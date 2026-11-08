use super::{assert_error, assert_ok};

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
    assert_ok("fn f() { let x: int; return; print(x); }");
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

#[test]
fn for_body_may_not_run() {
    assert_error(
        "fn f() { let x; for i in 0..3 { x = i; } print(x); }",
        UNINIT_X,
    );
    assert_ok("fn f() { let x; x = 0; for i in 0..3 { x = i; } print(x); }");
    assert_ok("fn f() { for i in 0..3 { let x; x = i; print(x); } }");
}

#[test]
fn for_bounds_and_variable() {
    assert_error("fn f() { let x: int; for i in 0..x { } }", UNINIT_X);
    assert_error("fn f() { let x: int; for i in x..3 { } }", UNINIT_X);
    assert_ok("fn f() { for i in 0..3 { print(i); } }");
}

#[test]
fn index_assignment_reads_the_array() {
    assert_error(
        "fn f() { let a: [int]; a[0] = 1; }",
        "`a` is used before being assigned",
    );
    assert_ok("fn f() { let a: [int]; a = [1]; a[0] = 2; }");
}

#[test]
fn reads_inside_arrays() {
    assert_error("fn f() { let x: int; print([x]); }", UNINIT_X);
    assert_error("fn f() { let x: int; print([0; x]); }", UNINIT_X);
    assert_error("fn f(a: [int]) { let x: int; print(a[x]); }", UNINIT_X);
    assert_error("fn f(a: [int]) { let x: int; a[x] = 1; }", UNINIT_X);
    assert_error("fn f(a: [int]) { let x: int; a[0] = x; }", UNINIT_X);
}

#[test]
fn for_each_body_may_not_run() {
    assert_error(
        "fn f(a: [int]) { let x; for y in a { x = y; } print(x); }",
        UNINIT_X,
    );
}
