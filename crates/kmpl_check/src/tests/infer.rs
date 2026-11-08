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

#[test]
fn vector_builtins() {
    assert_ok(
        "fn f(u: vec3, v: vec3) {
             let d: real = dot(u, v);
             let n: vec3 = normalize(cross(u, v));
             let m: vec3 = lerp(u, v, 0.5);
             let l: real = length(u) + distance(u, v);
         }",
    );
    assert_ok("fn f() { let x: real = lerp(0, 10, 0.5); let k: int = clamp(7, 0, 5); }");
    assert_error(
        "fn f(u: vec2, v: vec3) { let d = dot(u, v); }",
        "mismatched types: expected `vec2`, found `vec3`",
    );
    assert_error(
        "fn f(u: vec2, w: vec3) { let c = cross(u, w); }",
        "mismatched types: expected `vec3`, found `vec2`",
    );
    assert_error(
        "fn f(n: int) { let x = lerp(n, 1.0, 0.5); }",
        "mismatched types: expected `real`, `complex`, `vec2`, `vec3` or `vec4`, found `int`",
    );
}

#[test]
fn for_bounds_must_be_int() {
    assert_ok("fn f(n: int) { for i in 0..n { } for i in -1..n + 1 { } }");
    assert_error(
        "fn f() { for i in 0.0..3 { } }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error(
        "fn f(x: real) { for i in 0..x { } }",
        "mismatched types: expected `int`, found `real`",
    );
    assert_error(
        "fn f() { for i in 0..true { } }",
        "mismatched types: expected `int`, found `bool`",
    );
}

#[test]
fn for_variable_is_int() {
    assert_ok("fn f() { for i in 0..3 { let x: int = i; } }");
    assert_ok("fn f() { for i in 0..3 { let x: real = real(i); } }");
    assert_error(
        "fn f() { for i in 0..3 { let x: real = i; } }",
        "mismatched types: expected `real`, found `int`",
    );
}

#[test]
fn for_bound_narrows_a_flexible_variable() {
    assert_ok("fn f() { let n = 3; for i in 0..n { } let m: int = n; }");
    assert_error(
        "fn f() { let n = 3; for i in 0..n { } let m: real = n; }",
        "mismatched types: expected `real`, found `int`",
    );
}

#[test]
fn array_literals_have_one_element_type() {
    assert_ok("fn f() { let a = [1.0, 2.0]; let x: real = a[0]; }");
    assert_ok("fn f() { let a: [real] = [1, 2.0]; }");
    assert_ok("fn f() { let a = [1, 2]; let x: int = a[0]; }");
    assert_error(
        "fn f() { let a = [1, true]; }",
        "mismatched types: expected `int`, `real` or `complex`, found `bool`",
    );
    assert_error(
        "fn f() { let a: [int] = [1.5]; }",
        "mismatched types: expected `[int]`, found `[real]`",
    );
    assert_error(
        "fn f() { let a: [real] = 1.0; }",
        "mismatched types: expected `[real]`, found `real` or `complex`",
    );
}

#[test]
fn array_fill() {
    assert_ok("fn f(n: int) { let a: [real] = [0; n]; }");
    assert_error(
        "fn f() { let a = [0.0; 1.5]; }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
}

#[test]
fn indexing_needs_an_array_and_an_int() {
    assert_error("fn f(x: real) { x[0]; }", "expected an array, found `real`");
    assert_error("fn f(v: vec3) { v[0]; }", "expected an array, found `vec3`");
    assert_error(
        "fn f(a: [int]) { a[1.5]; }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error(
        "fn f(a: [int], x: real) { a[x] = 1; }",
        "mismatched types: expected `int`, found `real`",
    );
}

#[test]
fn index_assignment_types() {
    assert_ok("fn f(a: [real]) { a[0] = 1; }");
    assert_error(
        "fn f(a: [real]) { a[0] = true; }",
        "mismatched types: expected `real`, found `bool`",
    );
    assert_error(
        "fn f(x: real) { x[0] = 1.0; }",
        "expected an array, found `real`",
    );
    // Too many indices: an element is not an array.
    assert_error(
        "fn f(a: [real]) { a[0][1] = 1.0; }",
        "expected an array, found `real`",
    );
}

#[test]
fn nested_arrays() {
    assert_ok(
        "fn f() { let m = [[1.0], [2.0]]; let x: real = m[0][0]; m[1][0] = 3.0; m[0] = [4.0]; }",
    );
    assert_error(
        "fn f() { let m = [[1.0], 2.0]; }",
        "mismatched types: expected `[real]`, found `real` or `complex`",
    );
    assert_error(
        "fn f(m: [[real]]) { let a: [real] = m; }",
        "mismatched types: expected `[real]`, found `[[real]]`",
    );
}

#[test]
fn operators_on_arrays() {
    assert_ok("fn f(a: [real], b: [real]) { let same: bool = a == b; }");
    assert_error(
        "fn f(a: [real]) { let b = a + a; }",
        "cannot apply `+` to `[real]`",
    );
    assert_error(
        "fn f(a: [real]) { let b = -a; }",
        "cannot apply `-` to `[real]`",
    );
    assert_error(
        "fn f(a: [real], b: real) { let c = a * b; }",
        "cannot apply `*` to `[real]` and `real`",
    );
    assert_error(
        "fn f(a: [real], b: [real]) { let c = a < b; }",
        "cannot apply `<` to `[real]`",
    );
    assert_error(
        "fn f(a: [real]) { a:x; }",
        "swizzle needs a vector, found `[real]`",
    );
}

#[test]
fn empty_arrays_need_an_element_type() {
    assert_ok("fn f() { let a: [real] = []; }");
    assert_ok("fn f() { let a = []; a = [1.0]; }");
    assert_ok("fn g(a: [real]) { } fn f() { g([]); }");
    assert_error(
        "fn f() { let a = []; }",
        "cannot infer the element type of `a`, add a type annotation",
    );
    assert_error(
        "fn f() { print([]); }",
        "cannot infer the element type of this array",
    );
}

#[test]
fn a_type_cannot_contain_itself() {
    assert_error(
        "fn f() { let a = []; a = [a]; }",
        "mismatched types: expected `[_]`, found `[[_]]`",
    );
}

#[test]
fn arrays_in_signatures() {
    assert_ok("fn g(a: [real]) -> [real] { return a; } fn f() { let b: [real] = g([1, 2]); }");
    assert_error(
        "fn g(a: [real]) { } fn f() { g([true]); }",
        "mismatched types: expected `[real]`, found `[bool]`",
    );
    assert_error(
        "fn g() -> [real] { return [true]; }",
        "mismatched types: expected `[real]`, found `[bool]`",
    );
    assert_error(
        "fn g() -> [real] { return; }",
        "`g` must return a value of type `[real]`",
    );
    assert_ok("const A: [real] = [1, 2]; fn f() { let x: real = A[0]; }");
}

#[test]
fn len_takes_any_array() {
    assert_ok("fn f(a: [real], m: [[int]]) { let n: int = len(a) + len(m) + len(m[0]); }");
    assert_error(
        "fn f(x: real) { len(x); }",
        "expected an array, found `real`",
    );
}

#[test]
fn print_takes_arrays() {
    assert_ok("fn f(a: [real]) { print(a); print([1, 2]); }");
}

#[test]
fn for_each_variable_has_the_element_type() {
    assert_ok("fn f(a: [real]) { for x in a { let y: real = x; } }");
    assert_ok("fn f(m: [[int]]) { for row in m { for x in row { let y: int = x; } } }");
    assert_error(
        "fn f(a: [real]) { for x in a { let y: int = x; } }",
        "mismatched types: expected `int`, found `real`",
    );
    assert_error(
        "fn f() { for x in 1.5 { } }",
        "expected an array, found `real` or `complex`",
    );
}
