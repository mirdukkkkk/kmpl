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
fn length_takes_any_array() {
    assert_ok(
        "fn f(a: [real], m: [[int]]) { let n: int = length(a) + length(m) + m[0].length(); }",
    );
    assert_ok("fn f(v: vec3) { let n: real = length(v); }");
    assert_error(
        "fn f(x: real) { length(x); }",
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

#[test]
fn compound_assignment() {
    assert_ok(
        "fn f(x: real, v: vec2, a: [real]) { x += 1; x *= 2.0; v *= 2.0; v += v; a[0] /= 2; }",
    );
    assert_ok("fn f() { let x = 1; x += 2; let y: int = x; }");
    assert_error(
        "fn f(x: real) { x += true; }",
        "mismatched types: expected `real`, found `bool`",
    );
    assert_error(
        "fn f(x: int) { x /= 2.0; }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error("fn f(b: bool) { b += b; }", "cannot apply `+` to `bool`");
    // The result has to fit the variable.
    assert_error(
        "fn f(x: real, v: vec2) { x *= v; }",
        "mismatched types: expected `real`, found `vec2`",
    );
}

#[test]
fn range_step_is_an_int() {
    assert_ok("fn f(n: int) { for i in 0..=n step n { let j: int = i; } }");
    assert_error(
        "fn f() { for i in 0..3 step 1.5 { } }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
}

#[test]
fn slices_have_the_type_of_the_array() {
    assert_ok("fn f(a: [real], i: int) { let b: [real] = a[i..]; let c: [real] = a[..]; }");
    assert_ok("fn f(m: [[int]]) { let r: [[int]] = m[..=1]; let x: int = m[0..1][0][0]; }");
    assert_error(
        "fn f(a: [real]) { a[1.5..]; }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error(
        "fn f(x: real) { x[1..]; }",
        "expected an array, found `real`",
    );
}

#[test]
fn push_and_concat() {
    assert_ok("fn f(a: [real]) { let b: [real] = push(a, 1); let c: [real] = concat(a, b); }");
    assert_ok("fn f() { let m = [[1.0]]; m = push(m, [2.0]); let n: [[real]] = m; }");
    assert_ok("fn f() { let a = push([], 1.5); let b: [real] = a; }");
    assert_error(
        "fn f(a: [real]) { push(a, true); }",
        "mismatched types: expected `real`, found `bool`",
    );
    assert_error(
        "fn f(a: [real], b: [int]) { concat(a, b); }",
        "mismatched types: expected `[real]`, found `[int]`",
    );
    assert_error(
        "fn f(x: real) { push(x, 1.0); }",
        "mismatched types: expected `[_]`, found `real`",
    );
}

#[test]
fn element_wise_operators() {
    assert_ok("fn f(a: [real], b: [real]) { let c: [real] = a + b - a; let d: [real] = -a; }");
    assert_ok("fn f(m: [[real]], n: [[real]]) { let c: [[real]] = m + n; }");
    assert_ok("fn f(a: [vec2]) { let c: [vec2] = a + a; let d: [vec2] = a * 2.0; }");
    assert_ok(
        "fn f(a: [real]) { let b: [real] = a * 2; let c: [real] = 2 * a; let d: [real] = a / 2; }",
    );
    assert_ok("fn f(a: [int]) { let b: [int] = a * 2; }");
    assert_ok("fn f(m: [[real]]) { let b: [[real]] = m * 2.0; m *= 2; m += m; }");
    // A literal takes the type of the elements, or the other way around.
    assert_ok("fn f() { let a = [1, 2] * 2.5; let b: [real] = a; }");

    assert_error(
        "fn f(a: [bool]) { let b = a + a; }",
        "cannot apply `+` to `[bool]`",
    );
    assert_error(
        "fn f(a: [bool]) { let b = -a; }",
        "cannot apply `-` to `[bool]`",
    );
    assert_error(
        "fn f(a: [real], b: [int]) { a + b; }",
        "mismatched types: expected `[real]`, found `[int]`",
    );
    assert_error(
        "fn f(a: [real]) { let b = 2.0 / a; }",
        "cannot apply `/` to `real` or `complex` and `[real]`",
    );
    assert_error(
        "fn f(a: [real], b: [real]) { let c = a / b; }",
        "cannot apply `/` to `[real]` and `[real]`",
    );
    assert_error(
        "fn f(a: [real]) { let b = a * true; }",
        "cannot apply `*` to `real` and `bool`",
    );
}

#[test]
fn matrix_product() {
    assert_ok("fn f(m: [[real]], v: [real]) { let a: [[real]] = m * m; let b: [real] = m * v; }");
    assert_ok("fn f(m: [[real]], v: [real]) { m *= m; v = m * v; }");
    assert_ok("fn f(m: [[int]], n: [[complex]]) { let a = m * m; let b = n * n; }");
    assert_ok("fn f() { let a: [[real]] = [[1, 2]] * [[1.5], [2]]; }");

    assert_error(
        "fn f(a: [real], b: [real]) { let c = a * b; }",
        "cannot apply `*` to `[real]` and `[real]`",
    );
    assert_error(
        "fn f(m: [[real]], v: [real]) { let a = v * m; }",
        "cannot apply `*` to `[real]` and `[[real]]`",
    );
    assert_error(
        "fn f(m: [[bool]]) { let a = m * m; }",
        "cannot apply `*` to `bool`",
    );
    assert_error(
        "fn f(m: [[real]], n: [[int]]) { let a = m * n; }",
        "mismatched types: expected `real`, found `int`",
    );
    assert_error(
        "fn f(m: [[[real]]]) { let a = m * m; }",
        "cannot apply `*` to `[[[real]]]` and `[[[real]]]`",
    );
    assert_error(
        "fn f(m: [[real]], v: [real]) { m *= v; }",
        "mismatched types: expected `[[real]]`, found `[real]`",
    );
}

#[test]
fn tuple_types() {
    assert_ok("fn f() -> (int, real) { return (1, 2); }");
    assert_ok("fn f(t: (int, real)) { let (a, b) = t; let x: int = a; let y: real = b; }");
    assert_ok("fn f() { let t: (real, [int]) = (1, [2]); let (a, b) = t; let c: int = b[0]; }");
    assert_ok(
        "fn f(t: (int, (real, bool))) { let (a, inner) = t; let (b, c) = inner; let d: bool = c; }",
    );
    assert_ok("fn f(a: [(int, real)]) { for p in a { let (i, x) = p; let y: real = x; } }");
    assert_ok("fn f(a: (int, int), b: (int, int)) { let same: bool = a == b; print(a); }");
    assert_error(
        "fn f(p: (int, real)) { } fn g() { f((1.5, 1.5)); }",
        "mismatched types: expected `(int, real)`, found `(real, real)`",
    );
    assert_error(
        "fn f() -> (int, int) { return (1, 2, 3); }",
        "mismatched types: expected `(int, int)`, found `(int, int, int)`",
    );
}

#[test]
fn unpacking_checks_the_shape() {
    assert_error(
        "fn f() { let (a, b) = (1, 2, 3); }",
        "expected a tuple of 2 elements, found `(int, int, int)`",
    );
    assert_error(
        "fn f() { let (a, b) = 1; }",
        "expected a tuple of 2 elements, found `int`, `real` or `complex`",
    );
    assert_error(
        "fn f(a: [int]) { let (x, y) = a; }",
        "expected a tuple of 2 elements, found `[int]`",
    );
}

#[test]
fn tuples_have_no_arithmetic_or_indexing() {
    assert_error(
        "fn f(t: (int, int)) { t + t; }",
        "cannot apply `+` to `(int, int)`",
    );
    assert_error(
        "fn f(t: (int, int)) { t * 2; }",
        "cannot apply `*` to `(int, int)` and `int`, `real` or `complex`",
    );
    assert_error(
        "fn f(t: (int, int)) { t < t; }",
        "cannot apply `<` to `(int, int)`",
    );
    assert_error(
        "fn f(t: (int, int)) { t[0]; }",
        "expected an array, found `(int, int)`",
    );
    assert_error(
        "fn f(t: (int, int)) { for x in t { } }",
        "expected an array, found `(int, int)`",
    );
}

#[test]
fn tuple_element_types_must_be_known() {
    assert_error(
        "fn f() { let t = ([], 1); }",
        "cannot infer the element type of `t`, add a type annotation",
    );
    assert_error(
        "fn f() { let (a, b) = ([], 1); }",
        "cannot infer the element type of `a`, add a type annotation",
    );
}

#[test]
fn string_types() {
    assert_ok(r#"fn f() -> string { let s = "a"; let t: string = s + "b"; s += t; return s; }"#);
    assert_ok(
        r#"fn f(a: string, b: string) { let same: bool = a == b; let other: bool = a != b; }"#,
    );
    assert_ok(r#"fn f(a: [string]) { let b: [string] = a + a; let c: string = a[0]; }"#);
    assert_ok(r#"const S: string = "a" + "b";"#);
    assert_ok(r#"fn f() { let (a, b) = ("x", 1); let c: string = a; }"#);
    assert_error(
        r#"fn f() { let x = "a" + 1; }"#,
        "mismatched types: expected `string`, found `int`, `real` or `complex`",
    );
    assert_error(
        r#"fn f() { let x: int = "a"; }"#,
        "mismatched types: expected `int`, found `string`",
    );
    assert_error(
        r#"fn f() { if "a" { } }"#,
        "mismatched types: expected `bool`, found `string`",
    );
}

#[test]
fn strings_have_only_joining_and_equality() {
    assert_error(
        r#"fn f(a: string) { a - a; }"#,
        "cannot apply `-` to `string`",
    );
    assert_error(r#"fn f(a: string) { -a; }"#, "cannot apply `-` to `string`");
    assert_error(
        r#"fn f(a: string) { a * 2; }"#,
        "cannot apply `*` to `string` and `int`, `real` or `complex`",
    );
    assert_ok(r#"fn f(a: string) -> bool { return a < a; }"#);
    assert_ok(r#"fn f(a: string) { let c: string = a[0]; let p: string = a[1..]; }"#);
    assert_ok(r#"fn f(a: string) { for c in a { let s: string = c; } }"#);
    assert_error(
        r#"fn f(a: string) { a[0] = "x"; }"#,
        "expected an array, found `string`",
    );
    assert_error(
        r#"fn f(a: string) { a[true]; }"#,
        "mismatched types: expected `int`, found `bool`",
    );
}

#[test]
fn string_built_ins() {
    assert_ok(r#"fn f(s: string, a: [int]) { let n: int = length(s) + length(a) + length("x"); }"#);
    assert_ok(
        r#"fn f() { let s: string = string(1) + string(1.5) + string(true) + string([1]) + string("x"); }"#,
    );
    assert_error(
        "fn f(x: real) { length(x); }",
        "expected an array, found `real`",
    );
}

const POINT: &str = "struct P { x: real, n: int } ";

fn with_point(src: &str) -> String {
    format!("{POINT}{src}")
}

#[test]
fn struct_types() {
    assert_ok(&with_point(
        "fn f(p: P) -> real { let q: P = p; return q.x; }",
    ));
    assert_ok(&with_point(
        "fn f() { let p = P { x: 1, n: 2 }; let a: real = p.x; let b: int = p.n; }",
    ));
    assert_ok(&with_point(
        "fn f(a: [P]) { let x: real = a[0].x; for p in a { let y: int = p.n; } }",
    ));
    assert_ok(&with_point(
        "fn f(t: (P, int)) { let (p, _) = t; let x: real = p.x; }",
    ));
    assert_ok(&with_point(
        "fn f(a: P, b: P) { let same: bool = a == b; print(a); let s: string = string(a); }",
    ));
    assert_ok(&with_point("const O: P = P { x: 0, n: 0 };"));
    assert_ok("struct A { b: B } struct B { v: vec2 } fn f(a: A) { let s: vec2 = a.b.v:yx; }");
}

#[test]
fn struct_type_errors() {
    assert_error(
        &with_point("fn f() { P { x: true, n: 1 }; }"),
        "mismatched types: expected `real`, found `bool`",
    );
    assert_error(
        &with_point("fn f() { P { x: 1, n: 2.5 }; }"),
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error(&with_point("fn f(p: P) { p.z; }"), "`P` has no field `z`");
    assert_error(
        "struct P { } struct Q { } fn f(p: P) { let q: Q = p; }",
        "mismatched types: expected `Q`, found `P`",
    );
    assert_error(
        "fn f(x: real) { x.y; }",
        "only a struct has fields, found `real`",
    );
    assert_error(
        "fn f(a: [int]) { a.y; }",
        "only a struct has fields, found `[int]`",
    );
}

#[test]
fn field_assignment_types() {
    assert_ok(&with_point("fn f(p: P) { p.x = 1; p.x += 2; p.n *= 2; }"));
    assert_ok(&with_point(
        "fn f(a: [P]) { a[0].n = 1; a[0].x /= 2; a[1] = a[0]; }",
    ));
    assert_ok("struct A { v: [real] } fn f(a: A) { a.v[0] = 1; a.v += a.v; }");
    assert_error(
        &with_point("fn f(p: P) { p.n = 1.5; }"),
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error(
        &with_point("fn f(p: P) { p.z = 1; }"),
        "`P` has no field `z`",
    );
    assert_error(
        &with_point("fn f(x: int) { x.y = 1; }"),
        "only a struct has fields, found `int`",
    );
    assert_error(
        &with_point("fn f(a: [P], i: real) { a[i].n = 1; }"),
        "mismatched types: expected `int`, found `real`",
    );
}

#[test]
fn structs_have_no_arithmetic() {
    assert_error(
        &with_point("fn f(p: P) { p + p; }"),
        "cannot apply `+` to `P` and `P`",
    );
    assert_error(&with_point("fn f(p: P) { -p; }"), "cannot apply `-` to `P`");
    assert_error(
        &with_point("fn f(p: P) { p * 2.0; }"),
        "cannot apply `*` to `P` and `real` or `complex`",
    );
    assert_error(
        &with_point("fn f(p: P) { p < p; }"),
        "cannot apply `<` to `P` and `P`",
    );
    assert_error(
        &with_point("fn f(p: P) { p[0]; }"),
        "expected an array, found `P`",
    );
    assert_error(
        &with_point("fn f(p: P) { p:x; }"),
        "swizzle needs a vector, found `P`",
    );
}

#[test]
fn structs_infer_through_empty_arrays() {
    assert_ok(&with_point(
        "fn f(p: P) { let a = []; a = push(a, p); let n: int = a[0].n; }",
    ));
}

#[test]
fn input_built_ins() {
    assert_ok("fn f() { let a: [string] = _io_args(); let s: string = a[0]; }");
    assert_ok("fn f() { let line: string? = _io_read_line(); }");
    assert_ok(r#"fn f() { let n: int? = _text_parse_int("1"); }"#);
    assert_ok(r#"fn f() { let x: real? = _text_parse_real("1"); }"#);
    assert_error(
        "fn f() { _text_parse_int(1); }",
        "mismatched types: expected `string`, found `int`, `real` or `complex`",
    );
    assert_error(
        "fn f() { let (a, b, c) = _io_read_line(); }",
        "expected a tuple of 3 elements, found `string?`",
    );
}

const SHAPE: &str = "enum Shape { Circle(real), Rect(real, real), Empty } ";

fn with_shape(rest: &str) -> String {
    format!("{SHAPE}{rest}")
}

#[test]
fn enums_and_optionals_are_typed() {
    assert_ok(&with_shape(
        "fn f() { let s = Shape::Circle(1); let e: Shape = Shape::Empty; }",
    ));
    assert_ok(
        "fn f() { let a: real? = some(1); let b: int? = none; let c: [int?] = [none, some(1)]; }",
    );
    assert_ok("fn f(x: int?) -> bool { return x == none; }");
    assert_error(
        &with_shape("fn f() { let s = Shape::Circle(true); }"),
        "mismatched types: expected `real`, found `bool`",
    );
    assert_error(
        "fn f() { let a: int? = 1; }",
        "mismatched types: expected `int?`, found `int`, `real` or `complex`",
    );
    assert_error(
        "fn f() { print(none); }",
        "cannot infer the type of this `none`, add a type annotation",
    );
    assert_error(
        &with_shape("fn f(s: Shape) -> real { return s.x; }"),
        "only a struct has fields, found `Shape`",
    );
}

#[test]
fn match_binds_the_values_of_a_variant() {
    assert_ok(&with_shape(
        "fn f(s: Shape) -> real {
             match s {
                 Shape::Circle(r) => { return r; }
                 Shape::Rect(w, _) => { return w; }
                 Shape::Empty => { return 0.0; }
             }
         }",
    ));
    assert_ok(
        "fn f(x: int?) -> int { match x { some(v) => { return v; } none => { return 0; } } }",
    );
    assert_error(
        "fn f(x: int?) { match x { some(v) => { let s: string = v; } none => { } } }",
        "mismatched types: expected `string`, found `int`",
    );
}

#[test]
fn match_must_handle_every_case() {
    assert_error(
        &with_shape("fn f(s: Shape) { match s { Shape::Empty => { } } }"),
        "this `match` does not handle `Shape::Circle`, `Shape::Rect`; add it or `_`",
    );
    assert_error(
        "fn f(x: int?) { match x { none => { } } }",
        "this `match` does not handle `some`; add it or `_`",
    );
    assert_ok(&with_shape(
        "fn f(s: Shape) { match s { Shape::Empty => { } _ => { } } }",
    ));
}

#[test]
fn bad_match_arms() {
    assert_error(
        &with_shape(
            "fn f(s: Shape) { match s { Shape::Empty => { } Shape::Empty => { } _ => { } } }",
        ),
        "`Empty` is already handled above",
    );
    assert_error(
        &with_shape("fn f(s: Shape) { match s { _ => { } Shape::Empty => { } } }"),
        "this arm comes after `_`, so it can never run",
    );
    assert_error(
        &with_shape("fn f(s: Shape) { match s { none => { } _ => { } } }"),
        "this pattern does not fit a value of `Shape`",
    );
    assert_error(
        "fn f(x: int) { match x { _ => { } } }",
        "`match` needs an enum or an optional value, found `int`",
    );
}

#[test]
fn match_and_panic_end_paths() {
    assert_ok("fn f() -> int { panic(\"no\"); }");
    assert_ok(
        "fn f(x: int?) -> int { match x { some(v) => { return v; } none => { panic(\"no\"); } } }",
    );
    assert_error(
        "fn f(x: int?) -> int { match x { some(v) => { return v; } none => { } } }",
        "`f` may finish without returning a value",
    );
    assert_ok(
        "fn f(x: int?) { let y: int; match x { some(v) => { y = v; } none => { panic(\"no\"); } } print(y); }",
    );
    assert_error(
        "fn f(x: int?) { let y: int; match x { some(v) => { y = v; } none => { } } print(y); }",
        "`y` is used before being assigned",
    );
}

#[test]
fn matrix_cells_are_reals() {
    assert_ok("fn f(m: matrix) { let x: real = m[0, 1]; }");
    assert_ok("fn f(m: matrix) { m[0, 1] = 2; m[1, 1] += 3; }");
    assert_ok("fn f(m: matrix, i: int, j: int) { let x: real = m[i + 1, j * 2]; }");
    assert_error(
        "fn f(m: matrix) { let x: int = m[0, 0]; }",
        "mismatched types: expected `int`, found `real`",
    );
    assert_error(
        "fn f(m: matrix) { m[0, 0] = true; }",
        "mismatched types: expected `real`, found `bool`",
    );
    assert_error(
        "fn f(m: matrix) { let x = m[0.5, 0]; }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error(
        "fn f(m: matrix) { let x = m[0, 1.5]; }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
}

#[test]
fn matrix_cells_in_paths() {
    assert_ok(
        "struct S { m: matrix }
         fn f(s: S, list: [matrix], k: int) { s.m[0, 0] = 1.0; list[k][1, 1] += 2.0; let x = s.m[0, 1]; }",
    );
}

#[test]
fn a_matrix_takes_a_row_and_a_column() {
    assert_error(
        "fn f(m: matrix) { let r = m[0]; }",
        "a `matrix` takes a row and a column: `m[i, j]`",
    );
    assert_error(
        "fn f(m: matrix) { m[0] = 1.0; }",
        "a `matrix` takes a row and a column: `m[i, j]`",
    );
}

#[test]
fn only_a_matrix_takes_two_indices() {
    assert_error(
        "fn f(a: [[real]]) { let x = a[0, 1]; }",
        "only a `matrix` takes a row and a column, found `[[real]]`: write `a[i][j]`",
    );
    assert_error(
        "fn f(a: [[real]]) { a[0, 1] = 1.0; }",
        "only a `matrix` takes a row and a column, found `[[real]]`: write `a[i][j]`",
    );
}

#[test]
fn matrix_arithmetic() {
    assert_ok("fn f(a: matrix, b: matrix) { let c: matrix = a + b - a; let d: matrix = -a; }");
    assert_ok("fn f(a: matrix, b: matrix) { let c: matrix = a * b; }");
    assert_ok("fn f(a: matrix, v: [real]) { let c: [real] = a * v; }");
    assert_ok(
        "fn f(a: matrix, k: real) { let b: matrix = a * k; let c: matrix = k * a; let d: matrix = a / k; }",
    );
    assert_ok(
        "fn f(a: matrix) { let b: matrix = a * 2; let c: matrix = 2 * a; let d: matrix = a / 2; }",
    );
    assert_ok("fn f(a: matrix, b: matrix) { let p: bool = a == b; let q = a != b; }");
    assert_ok("fn f(a: matrix) { a += a; a *= 2.0; a /= 2.0; }");
}

#[test]
fn matrix_arithmetic_errors() {
    assert_error(
        "fn f(a: matrix, v: [real]) { let c = v * a; }",
        "cannot apply `*` to `[real]` and `matrix`",
    );
    assert_error(
        "fn f(a: matrix, b: matrix) { let c = a / b; }",
        "cannot apply `/` to `matrix` and `matrix`",
    );
    assert_error(
        "fn f(a: matrix) { let c = a + 1.0; }",
        "mismatched types: expected `matrix`, found `real` or `complex`",
    );
    assert_error(
        "fn f(a: matrix) { let c = a + 1; }",
        "mismatched types: expected `matrix`, found `int`, `real` or `complex`",
    );
    assert_error(
        "fn f(a: matrix, v: vec3) { let c = a * v; }",
        "cannot apply `*` to `matrix` and `vec3`",
    );
    assert_error(
        "fn f(a: matrix, k: int) { let c = a * k; }",
        "mismatched types: expected `real`, found `int`",
    );
    assert_error(
        "fn f(a: matrix, b: matrix) { let c = a < b; }",
        "cannot apply `<` to `matrix`",
    );
}

#[test]
fn matrix_built_ins() {
    assert_ok(
        "fn f() {
             let m: matrix = matrix(2, 3);
             let r: int = rows(m); let c: int = cols(m);
             let a: [[real]] = to_rows(m);
             let n: matrix = to_matrix(a);
             let k: matrix = to_matrix([[1, 2], [3, 4]]);
         }",
    );
    assert_error(
        "fn f() { let m = matrix(2.5, 1); }",
        "mismatched types: expected `int`, found `real` or `complex`",
    );
    assert_error(
        "fn f(a: [[int]]) { let m = to_matrix(a); }",
        "mismatched types: expected `[[real]]`, found `[[int]]`",
    );
    assert_error(
        "fn f(a: [real]) { let m = rows(a); }",
        "mismatched types: expected `matrix`, found `[real]`",
    );
}

#[test]
fn matrices_in_signatures_structs_and_arrays() {
    assert_ok("fn g(m: matrix) -> matrix { return m; } fn f(m: matrix) { let n: matrix = g(m); }");
    assert_ok(
        "struct S { m: matrix } fn f() { let s = S { m: matrix(1, 1) }; let x: real = s.m[0, 0]; }",
    );
    assert_ok("fn f(a: [matrix], b: [matrix]) { let c: [matrix] = a + b; let d: matrix = a[0]; }");
    assert_ok(
        "fn f() { let a = [matrix(1, 1), matrix(2, 2)]; let b: [matrix] = push(a, matrix(0, 0)); }",
    );
    assert_ok("fn f(m: matrix) -> matrix? { return some(m); }");
}

#[test]
fn matrices_in_generics() {
    assert_ok(
        "fn id<T>(x: T) -> T { return x; }
         fn f(m: matrix) { let n: matrix = id(m); let k: real = id(m)[0, 0]; }",
    );
}

const VEC: &str = "struct V { x: real, y: real }
    operator +(a: V, b: V) -> V { return V { x: a.x + b.x, y: a.y + b.y }; }
    operator -(a: V, b: V) -> V { return V { x: a.x - b.x, y: a.y - b.y }; }
    operator -(a: V) -> V { return V { x: -a.x, y: -a.y }; }
    operator *(k: real, a: V) -> V { return V { x: k * a.x, y: k * a.y }; }
    operator *(a: V, k: real) -> V { return k * a; }
    operator *(a: V, b: V) -> real { return a.x * b.x + a.y * b.y; }
    operator <(a: V, b: V) -> bool { return a.x < b.x; }
    ";

fn with_vec(src: &str) -> String {
    format!("{VEC}{src}")
}

#[test]
fn operators_give_the_type_of_their_definition() {
    assert_ok(&with_vec("fn f(a: V, b: V) -> V { return a + b; }"));
    assert_ok(&with_vec("fn f(a: V, b: V) -> V { return a - b; }"));
    assert_ok(&with_vec("fn f(a: V) -> V { return -a; }"));
    assert_ok(&with_vec("fn f(a: V, b: V) -> real { return a * b; }"));
    assert_ok(&with_vec("fn f(a: V, b: V) -> bool { return a < b; }"));
    assert_ok(&with_vec("fn f(a: V, b: V) -> V { return (a + b) * 2.0; }"));
    assert_error(
        &with_vec("fn f(a: V, b: V) -> V { return a * b; }"),
        "mismatched types: expected `V`, found `real`",
    );
    assert_error(
        &with_vec("fn f(a: V, b: V) -> real { return a + b; }"),
        "mismatched types: expected `real`, found `V`",
    );
    assert_error(
        &with_vec("fn f(a: V, b: V) { let x: int = a < b; }"),
        "mismatched types: expected `int`, found `bool`",
    );
}

#[test]
fn mixed_type_operators_work_in_both_orders() {
    assert_ok(&with_vec(
        "fn f(a: V, k: real) -> V { let x = k * a; let y = a * k; return x + y; }",
    ));
    assert_ok(&with_vec("fn f(a: V) -> V { return 2.0 * a; }"));
    // Only `V * real` and `real * V` exist, not `V + real`.
    assert_error(
        &with_vec("fn f(a: V, k: real) -> V { return a + k; }"),
        "cannot apply `+` to `V` and `real`",
    );
    assert_error(
        &with_vec("fn f(a: V, k: real) -> V { return k + a; }"),
        "cannot apply `+` to `real` and `V`",
    );
    assert_error(
        &with_vec("fn f(a: V, k: int) -> V { return k * a; }"),
        "cannot apply `*` to `int` and `V`",
    );
}

#[test]
fn no_fitting_definition() {
    assert_error(
        &with_vec("fn f(a: V, b: V) -> bool { return a > b; }"),
        "cannot apply `>` to `V` and `V`",
    );
    assert_error(
        &with_vec("fn f(a: V, b: V) -> V { return a / b; }"),
        "cannot apply `/` to `V` and `V`",
    );
    assert_error(
        &with_vec("fn f(a: V, b: V) -> V { return a % b; }"),
        "cannot apply `%` to `V` and `V`",
    );
    assert_error(
        "struct W { x: real } fn f(a: W) -> W { return -a; }",
        "cannot apply `-` to `W`",
    );
    assert_error(
        &with_vec("struct W { x: real } fn f(a: V, b: W) -> V { return a + b; }"),
        "cannot apply `+` to `V` and `W`",
    );
}

#[test]
fn one_error_for_a_missing_operator() {
    let errors = errors(&with_vec(
        "fn f(a: V, b: V) { let x = a / b; let y = x + 1.0; }",
    ));
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn equality_stays_structural_and_the_rest_cannot_be_defined() {
    assert_ok(&with_vec(
        "fn f(a: V, b: V) -> bool { return a == b || a != b; }",
    ));
    // A literal that could be a real or a complex.
    assert_error(
        &with_vec("fn f(a: V) -> V { return a + 1.0; }"),
        "cannot apply `+` to `V` and `real` or `complex`",
    );
}

#[test]
fn a_number_literal_takes_its_default_among_definitions() {
    let defs = "struct V { x: real }
        operator *(k: int, a: V) -> int { return k; }
        operator *(k: real, a: V) -> real { return a.x; }
        operator *(k: complex, a: V) -> bool { return true; }
        ";
    // `2` is an `int`, `2.0` a `real`, `2.0i` a `complex`.
    assert_ok(&format!("{defs}fn f(v: V) -> int {{ return 2 * v; }}"));
    assert_ok(&format!("{defs}fn f(v: V) -> real {{ return 2.0 * v; }}"));
    assert_ok(&format!("{defs}fn f(v: V) -> bool {{ return 2.0i * v; }}"));
    assert_error(
        &format!("{defs}fn f(v: V) -> real {{ return 2 * v; }}"),
        "mismatched types: expected `real`, found `int`",
    );
    // With only the `real` definition an integer literal becomes a real.
    assert_ok(
        "struct V { x: real } operator *(k: real, a: V) -> real { return k; }
         fn f(v: V) -> real { return 2 * v; }",
    );
}

#[test]
fn a_definition_that_cannot_fit_is_not_chosen_for_literals() {
    assert_error(
        "struct V { x: real } operator *(k: int, a: V) -> int { return k; }
         fn f(v: V) -> int { return 2.0 * v; }",
        "cannot apply `*` to `real` or `complex` and `V`",
    );
}

#[test]
fn an_open_operand_can_make_the_choice_ambiguous() {
    let defs = "struct V { x: real }
        operator *(a: V, k: int) -> int { return k; }
        operator *(a: V, k: real) -> real { return a.x; }
        ";
    assert_error(
        &format!("{defs}fn f(v: V) {{ let a = []; let r = v * a[0]; }}"),
        "several definitions of operator `*` fit `V` and any type, add a type annotation",
    );
}

#[test]
fn comparison_operators_work_in_conditions() {
    assert_ok(&with_vec(
        "fn f(a: V, b: V) { if a < b { } while a < b { } let c = a < b && b < a; }",
    ));
    assert_ok(
        "enum Level { Low, High }
         operator <(a: Level, b: Level) -> bool { return true; }
         operator >=(a: Level, b: Level) -> bool { return true; }
         fn f(a: Level, b: Level) -> bool { return a < b || a >= b; }",
    );
}

#[test]
fn comparison_operators_may_give_other_types() {
    assert_ok(
        "struct V { x: real }
         operator <(a: V, b: V) -> real { return a.x - b.x; }
         fn f(a: V, b: V) -> real { return a < b; }",
    );
    assert_error(
        "struct V { x: real }
         operator <(a: V, b: V) -> real { return a.x - b.x; }
         fn f(a: V, b: V) { if a < b { } }",
        "mismatched types: expected `bool`, found `real`",
    );
}

#[test]
fn compound_assignment_uses_the_operator() {
    assert_ok(&with_vec(
        "struct H { v: V } fn f(a: V, b: V, h: H, list: [V]) {
            a += b; a -= b; a *= 2.0;
            h.v += b;
            list[0] += b;
            list[0].x += 1.0;
            h.v.x *= 2.0;
        }",
    ));
}

#[test]
fn compound_assignment_result_must_fit_the_target() {
    assert_error(
        &with_vec("fn f(a: V, b: V) { a *= b; }"),
        "mismatched types: expected `V`, found `real`",
    );
    assert_error(
        &with_vec("struct H { v: V } fn f(h: H, b: V) { h.v *= b; }"),
        "mismatched types: expected `V`, found `real`",
    );
    assert_error(
        &with_vec("fn f(list: [V], b: V) { list[0] *= b; }"),
        "mismatched types: expected `V`, found `real`",
    );
    assert_error(
        &with_vec("fn f(a: V) { a /= a; }"),
        "cannot apply `/` to `V` and `V`",
    );
}

#[test]
fn operators_on_enums_and_generic_structs() {
    assert_ok(
        "enum Money { Euro(int), Free }
         operator +(a: Money, b: Money) -> Money { return a; }
         operator -(a: Money) -> Money { return a; }
         fn f(a: Money, b: Money) -> Money { let c = a + b; c += a; return -c; }",
    );
    assert_ok(
        "struct Pair<T> { a: T, b: T }
         operator +(p: Pair<real>, q: Pair<real>) -> Pair<real> { return p; }
         fn f(p: Pair<real>) -> Pair<real> { return p + p; }",
    );
    // A definition for `Pair<real>` does not cover `Pair<int>`.
    assert_error(
        "struct Pair<T> { a: T, b: T }
         operator +(p: Pair<real>, q: Pair<real>) -> Pair<real> { return p; }
         fn f(p: Pair<int>) -> Pair<int> { return p + p; }",
        "cannot apply `+` to `Pair<int>` and `Pair<int>`",
    );
    assert_ok(
        "struct Pair<T> { a: T, b: T }
         operator +(p: Pair<real>, q: Pair<real>) -> Pair<real> { return p; }
         operator +(p: Pair<int>, q: Pair<int>) -> Pair<int> { return p; }
         fn f(p: Pair<int>, q: Pair<real>) { let a = p + p; let b = q + q; }",
    );
}

#[test]
fn an_operator_body_may_use_other_operators_and_itself() {
    assert_ok(&with_vec(
        "operator +(a: V, k: real) -> V { return a + V { x: k, y: k }; }
         operator /(a: V, k: real) -> V { return (1.0 / k) * a; }
         fn f(a: V) -> V { return a + 1.0; }",
    ));
    assert_ok(
        "enum N { Z, S(N) }
         operator +(a: N, b: N) -> N {
             match b { N::Z => { return a; } N::S(p) => { return N::S(a + p); } }
         }",
    );
}

#[test]
fn precedence_is_not_changed_by_operators() {
    assert_ok(&with_vec(
        "fn f(a: V, b: V) -> V { return a + 2.0 * b - -a; }",
    ));
    assert_error(
        &with_vec("fn f(a: V, b: V) -> V { return a + b * b; }"),
        "cannot apply `+` to `V` and `real`",
    );
}

#[test]
fn operators_in_constants() {
    assert_ok(&with_vec(
        "const A: V = V { x: 1.0, y: 2.0 }; const B: V = A + A * 2.0; const C: V = -B;",
    ));
}

#[test]
fn builtin_arithmetic_is_not_replaced() {
    assert_ok(&with_vec(
        "fn f() -> real { return 1.0 + 2.0 * 3.0; } fn g(a: vec2) -> vec2 { return a + a; }",
    ));
}
