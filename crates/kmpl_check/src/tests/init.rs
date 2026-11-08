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
