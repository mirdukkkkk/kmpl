use super::{assert_error, assert_ok, errors};

#[test]
fn flexible_literals() {
    assert_ok("fn f() { let x: real = 1; let z: complex = -1; let c: complex = 2.0; }");
    assert_ok("fn f(x: real) { let y = 2 * x + 1; let h = x / 2; }");
    assert_ok("fn f() { let z = 2.0 + 3.0i; let w = 1 + 1.0i; }");
    assert_error(
        "fn f() { let x: int = 1.5; }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error(
        "fn f() { let x: real = 1.0i; }",
        "mismatched types: expected `real`, found `complex`",
    );
}

#[test]
fn inference_flows_through_variables() {
    assert_ok("fn f() { let x = 1; let y: real = x; }");
    assert_ok("fn f() { let x; x = 2.0; let y: complex = x; }");
    assert_error(
        "fn f() { let x: real = 1.0; let y: int = x; }",
        "mismatched types: expected `int`, found `real`",
    );
}

#[test]
fn assignment_keeps_the_type() {
    assert_error(
        "fn f() { let x = 1.0; x = true; }",
        "mismatched types: expected `real` or `complex`, found `bool`",
    );
    assert_error(
        "fn f(c: bool) { let x; if c { x = 1.5; } else { x = true; } print(x); }",
        "mismatched types: expected `real` or `complex`, found `bool`",
    );
}

#[test]
fn arithmetic() {
    assert_ok("fn f(a: int, b: real, c: complex) { let x = a + a; let y = b - b; let z = c * c; }");
    assert_ok("fn f(u: vec3, v: vec3) { let w = u + v - u; let n = -u; }");
    assert_error(
        "fn f(a: int, b: real) { let x = a + b; }",
        "mismatched types: expected `int`, found `real`",
    );
    assert_error(
        "fn f(u: vec2, v: vec3) { let w = u + v; }",
        "mismatched types: expected `vec2`, found `vec3`",
    );
    assert_error(
        "fn f() { let x = true + false; }",
        "cannot apply `+` to `bool`",
    );
    assert_error("fn f() { let x = -true; }", "cannot apply `-` to `bool`");
}

#[test]
fn vector_scaling() {
    assert_ok("fn f(v: vec3) { let a = v * 2.0; let b = 2.0 * v; let c = v / 2; }");
    assert_ok("fn f(v: vec4, k: real) { let a: vec4 = k * v * k; }");
    assert_error(
        "fn f(v: vec3, n: int) { let a = v * n; }",
        "mismatched types: expected `real`, found `int`",
    );
    assert_error(
        "fn f(u: vec3, v: vec3) { let a = u * v; }",
        "cannot apply `*` to `vec3` and `vec3`",
    );
    assert_error(
        "fn f(v: vec2) { let a = 2.0 / v; }",
        "cannot apply `/` to `real` or `complex` and `vec2`",
    );
}

#[test]
fn comparisons_and_logic() {
    assert_ok("fn f(a: int, b: real) { let p = a < 1; let q = b >= 0.5; let r = p && q || !p; }");
    assert_ok("fn f(u: vec3, z: complex) { let p = u == u; let q = z != 1.0i; }");
    assert_error(
        "fn f(z: complex) { let p = z < z; }",
        "cannot apply `<` to `complex`",
    );
    assert_error(
        "fn f(a: int) { let p = a && true; }",
        "mismatched types: expected `bool`, found `int`",
    );
    assert_error(
        "fn f() { let p = !1; }",
        "mismatched types: expected `bool`, found `int`, `real` or `complex`",
    );
}

#[test]
fn conditions_must_be_bool() {
    assert_error(
        "fn f(x: real) { if x { } }",
        "mismatched types: expected `bool`, found `real`",
    );
    assert_error(
        "fn f() { while 1 { } }",
        "mismatched types: expected `bool`, found `int`, `real` or `complex`",
    );
}

#[test]
fn user_function_calls() {
    assert_ok(
        "fn g(a: real, b: vec2) -> real { return a; } fn f() { let x: real = g(1, vec2(1, 2)); }",
    );
    assert_error(
        "fn g(a: bool) { } fn f() { g(1); }",
        "mismatched types: expected `bool`, found `int`, `real` or `complex`",
    );
    assert_error(
        "fn g() -> int { return 1; } fn f() { let x: real = g(); }",
        "mismatched types: expected `real`, found `int`",
    );
}

#[test]
fn generic_builtins() {
    assert_ok("fn f() { let a: real = sqrt(2); let b: complex = sqrt(-1 + 0.0i); }");
    assert_ok("fn f(z: complex) { let w: complex = exp(z) * log(z); }");
    assert_ok("fn f() { let a: int = abs(-3); let b: real = abs(-2.5); }");
    assert_error(
        "fn f() { let x: int = sqrt(4); }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error(
        "fn f(v: vec2) { let a = sin(v); }",
        "mismatched types: expected `real` or `complex`, found `vec2`",
    );
}

#[test]
fn functions_without_value() {
    assert_ok("fn g() { } fn f() { g(); print(1); }");
    assert_error("fn f() { let x = print(1); }", "`print` returns no value");
    assert_error(
        "fn g() { } fn f() { let x = 1 + g(); }",
        "`g` returns no value",
    );
}

#[test]
fn swizzle() {
    assert_ok("fn f(v: vec4) { let a: real = v:w; let b: vec2 = v:xy; let c: vec4 = v:wzyx; }");
    assert_ok("fn f(v: vec2) { let a: vec4 = v:xyxy; let b: vec3 = v:yyx; }");
    assert_error(
        "fn f(v: vec2) { let a = v:z; }",
        "invalid swizzle `z` for `vec2`: use 1 to 4 of the letters `xy`",
    );
    assert_error(
        "fn f(v: vec3) { let a = v:xyzxy; }",
        "invalid swizzle `xyzxy` for `vec3`: use 1 to 4 of the letters `xyz`",
    );
    assert_error(
        "fn f(x: real) { let a = x:x; }",
        "swizzle needs a vector, found `real`",
    );
}

#[test]
fn no_implicit_real_to_complex_for_typed_values() {
    assert_error(
        "fn f(x: real) { let z = x * 2.0i; }",
        "mismatched types: expected `real`, found `complex`",
    );
}

#[test]
fn constants() {
    assert_ok("const N: real = 1; const M: complex = 2 * 2.0i; fn f() { let x: real = N; }");
    assert_error(
        "const N: int = true;",
        "mismatched types: expected `int`, found `bool`",
    );
}

#[test]
fn unknown_types() {
    assert_error(
        "fn f() { let x; }",
        "cannot infer the type of `x`, add a type annotation",
    );
    assert_ok("fn f() { let x: vec3; }");
    assert_ok("fn f() { let x = 1; print(x); }");
}

#[test]
fn all_type_errors_are_reported() {
    assert_eq!(
        errors("fn f(x: real) { let a: int = x; if x { } let b = !x; }"),
        vec![
            "mismatched types: expected `int`, found `real`",
            "mismatched types: expected `bool`, found `real`",
            "mismatched types: expected `bool`, found `real`",
        ]
    );
}

#[test]
fn conversions_bridge_typed_values() {
    assert_ok("fn f(x: real) { let z = complex(x, 0) * 2.0i; }");
    assert_ok("fn f(n: int) { let half = real(n) / 2; let back: int = int(half); }");
    assert_ok("fn f(z: complex) { let r: real = re(z) + im(z); }");
    assert_error(
        "fn f(z: complex) { let r = real(z); }",
        "mismatched types: expected `int` or `real`, found `complex`",
    );
    assert_error(
        "fn f(n: int) { let z = complex(n, 0); }",
        "mismatched types: expected `real`, found `int`",
    );
}

#[test]
fn abs_of_complex_is_real() {
    assert_ok("fn f(z: complex) { let r: real = abs(z); }");
    assert_ok(
        "fn f() { let n: int = abs(-3); let x: real = abs(-2.5); let r: real = abs(3.0 + 4.0i); }",
    );
    assert_ok("fn f(z: complex) { let a: real = arg(z); let w: complex = conj(z); }");
    // An open literal never picks the complex reading.
    assert_error(
        "fn f() { let w: complex = abs(-1); }",
        "mismatched types: expected `complex`, found `int` or `real`",
    );
}
