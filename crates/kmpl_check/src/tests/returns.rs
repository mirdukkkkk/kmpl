use super::{assert_error, assert_ok};

#[test]
fn return_value_type() {
    assert_ok("fn f() -> real { return 1; }");
    assert_ok("fn f(v: vec3) -> vec3 { return v * 2; }");
    assert_error(
        "fn f() -> int { return true; }",
        "mismatched types: expected `int`, found `bool`",
    );
    assert_error(
        "fn f(x: real) -> int { return x; }",
        "mismatched types: expected `int`, found `real`",
    );
}

#[test]
fn return_value_from_function_without_type() {
    assert_error(
        "fn f() { return 1; }",
        "`f` returns nothing, but `return` has a value",
    );
    assert_error(
        "fn main() { return 0; }",
        "`main` returns nothing, but `return` has a value",
    );
    assert_ok("fn f() { return; }");
}

#[test]
fn bare_return_in_function_with_type() {
    assert_error(
        "fn f() -> real { return; }",
        "`f` must return a value of type `real`",
    );
}

#[test]
fn every_path_must_return() {
    assert_error(
        "fn f() -> int { }",
        "`f` may finish without returning a value",
    );
    assert_error(
        "fn f(c: bool) -> int { if c { return 1; } }",
        "`f` may finish without returning a value",
    );
    assert_error(
        "fn f(c: bool) -> int { if c { return 1; } else { } }",
        "`f` may finish without returning a value",
    );
    assert_ok("fn f(c: bool) -> int { if c { return 1; } else { return 2; } }");
    assert_ok("fn f(c: bool) -> int { if c { return 1; } return 2; }");
}

#[test]
fn else_if_chain_returns() {
    assert_ok(
        "fn sign(x: real) -> real {
             if x > 0.0 { return 1.0; } else if x < 0.0 { return -1.0; } else { return 0.0; }
         }",
    );
    assert_error(
        "fn sign(x: real) -> real {
             if x > 0.0 { return 1.0; } else if x < 0.0 { return -1.0; }
         }",
        "`sign` may finish without returning a value",
    );
}

#[test]
fn while_does_not_count_as_returning() {
    assert_error(
        "fn f() -> int { while true { return 1; } }",
        "`f` may finish without returning a value",
    );
    assert_ok("fn f() -> int { while true { return 1; } return 0; }");
}

#[test]
fn functions_without_type_need_no_return() {
    assert_ok("fn f(c: bool) { if c { return; } }");
}

#[test]
fn for_does_not_count_as_returning() {
    assert_error(
        "fn f() -> int { for i in 0..3 { return i; } }",
        "`f` may finish without returning a value",
    );
    assert_ok("fn f() -> int { for i in 0..3 { return i; } return 0; }");
}

#[test]
fn for_each_does_not_count_as_returning() {
    assert_error(
        "fn f(a: [int]) -> int { for x in a { return x; } }",
        "`f` may finish without returning a value",
    );
}
