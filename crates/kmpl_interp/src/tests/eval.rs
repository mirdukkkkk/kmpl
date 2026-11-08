use super::{output, run, runtime_error};

#[test]
fn print_values() {
    assert_eq!(
        output("print(1); print(1.5); print(true);"),
        "1\n1.5\ntrue\n"
    );
    assert_eq!(output("print(2.0 + 3.0i);"), "2.0+3.0i\n");
    assert_eq!(output("print(vec3(1, 2, 3));"), "(1.0, 2.0, 3.0)\n");
}

#[test]
fn flexible_literals_run_with_their_type() {
    assert_eq!(output("let x: real = 1; print(x / 2);"), "0.5\n");
    assert_eq!(output("let x = 7; print(x / 2);"), "3\n");
    assert_eq!(output("let z: complex = -1; print(sqrt(z));"), "0.0+1.0i\n");
    assert_eq!(output("print(sqrt(-1));"), "NaN\n");
}

#[test]
fn variables_and_scopes() {
    assert_eq!(
        output("let x = 1; if true { let x = 2; print(x); } print(x);"),
        "2\n1\n"
    );
    assert_eq!(
        output("let x; if true { x = 5; } else { x = 6; } print(x);"),
        "5\n"
    );
    // Assignment updates the innermost variable with that name.
    assert_eq!(
        output("let x = 1; while true { let x = 2; x = 3; break; } print(x);"),
        "1\n"
    );
}

#[test]
fn if_else_chain() {
    let src = "fn sign(x: real) -> int {
                   if x > 0 { return 1; } else if x < 0 { return -1; } else { return 0; }
               }
               fn main() { print(sign(2.5)); print(sign(-1)); print(sign(0)); }";
    assert_eq!(run(src).unwrap(), "1\n-1\n0\n");
}

#[test]
fn while_with_break_and_continue() {
    assert_eq!(
        output(
            "let i = 0; let sum = 0;
             while true {
                 i = i + 1;
                 if i > 10 { break; }
                 if i / 2 * 2 == i { continue; }
                 sum = sum + i;
             }
             print(sum);"
        ),
        "25\n"
    );
}

#[test]
fn return_from_inside_loop() {
    let src = "fn first_square_over(n: int) -> int {
                   let i = 0;
                   while true { if i * i > n { return i; } i = i + 1; }
                   return -1;
               }
               fn main() { print(first_square_over(50)); }";
    assert_eq!(run(src).unwrap(), "8\n");
}

#[test]
fn recursion() {
    let src = "fn fact(n: int) -> int { if n <= 1 { return 1; } return n * fact(n - 1); }
               fn fib(n: int) -> int { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }
               fn main() { print(fact(20)); print(fib(20)); }";
    assert_eq!(run(src).unwrap(), "2432902008176640000\n6765\n");
}

#[test]
fn functions_without_value_and_bare_return() {
    let src = "fn show(x: int) { if x < 0 { return; } print(x); }
               fn main() { show(-1); show(2); }";
    assert_eq!(run(src).unwrap(), "2\n");
}

#[test]
fn constants() {
    let src = "const N: int = 3; const HALF: real = 1 / 2.0; const Z: complex = 0.5 * 2.0i;
               fn main() { print(N * N); print(HALF); print(Z); }";
    assert_eq!(run(src).unwrap(), "9\n0.5\n0.0+1.0i\n");
}

#[test]
fn short_circuit() {
    let src = "fn loud() -> bool { print(1); return true; }
               fn main() {
                   let a = false && loud();
                   let b = true || loud();
                   let c = true && loud();
                   print(a); print(b); print(c);
               }";
    assert_eq!(run(src).unwrap(), "1\nfalse\ntrue\ntrue\n");
}

#[test]
fn vectors_and_swizzles() {
    assert_eq!(
        output("let v = vec3(1, 2, 3); print(v:zyx); print(v:x); print(2 * v - v);"),
        "(3.0, 2.0, 1.0)\n1.0\n(1.0, 2.0, 3.0)\n"
    );
}

#[test]
fn integer_division_by_zero() {
    let src = "fn main() { let z = 0; print(1); print(10 / z); print(2); }";
    let (message, printed) = run(src).unwrap_err();
    assert_eq!(message, "division by zero");
    assert_eq!(printed, "1\n");
}

#[test]
fn integer_overflow() {
    assert_eq!(
        runtime_error("fn main() { let x = 9223372036854775807; print(x + 1); }"),
        "integer overflow"
    );
}

#[test]
fn real_division_by_zero_is_not_an_error() {
    assert_eq!(output("print(1.0 / 0); print(-1.0 / 0);"), "inf\n-inf\n");
}

#[test]
fn runtime_error_span_points_at_the_operation() {
    let src = "fn main() { let z = 0; let y = 10 / z; }";
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    kmpl_check::check(&mut program).unwrap();
    let err = crate::run(&program, &mut Vec::new()).unwrap_err();
    assert_eq!(&src[err.span.start..err.span.end], "10 / z");
}

#[test]
fn deep_recursion_is_an_error_not_a_crash() {
    let src = "fn down(n: int) -> int { return down(n + 1); } fn main() { print(down(0)); }";
    assert_eq!(
        runtime_error(src),
        format!(
            "stack overflow: more than {} nested calls",
            crate::MAX_CALL_DEPTH
        )
    );
}

#[test]
fn recursion_just_under_the_limit_works() {
    let depth = crate::MAX_CALL_DEPTH - 1;
    let src = format!(
        "fn down(n: int) -> int {{ if n == 0 {{ return 0; }} return 1 + down(n - 1); }}
         fn main() {{ print(down({})); }}",
        depth - 1
    );
    assert_eq!(run(&src).unwrap(), format!("{}\n", depth - 1));
}

#[test]
fn negative_complex_has_no_negative_zero() {
    assert_eq!(output("print(-2.0i);"), "0.0-2.0i\n");
    assert_eq!(
        output("let z: complex = -1; print(z); print(log(z));"),
        "-1.0+0.0i\n0.0+3.141592653589793i\n"
    );
}

#[test]
fn conversions() {
    assert_eq!(
        output("let n = 7; print(real(n) / 2); print(int(-2.7)); print(complex(1, -2));"),
        "3.5\n-2\n1.0-2.0i\n"
    );
    assert_eq!(
        output("let z = 3.0 + 4.0i; print(re(z)); print(im(z));"),
        "3.0\n4.0\n"
    );
}

#[test]
fn int_of_nan_is_a_runtime_error() {
    assert_eq!(
        runtime_error("fn main() { print(int(sqrt(-1))); }"),
        "cannot convert NaN to `int`"
    );
}

#[test]
fn complex_helpers() {
    assert_eq!(
        output(
            "let z = 3.0 - 4.0i; print(abs(z)); print(conj(z)); print(abs(-7)); print(arg(-1.0 + 0.0i));"
        ),
        "5.0\n3.0+4.0i\n7\n3.141592653589793\n"
    );
}

#[test]
fn vector_builtins() {
    assert_eq!(
        output(
            "let u = vec3(1, 0, 0); let v = vec3(0, 1, 0);
             print(cross(u, v)); print(dot(u, v)); print(length(vec2(3, 4)));
             print(lerp(u, v, 0.5)); print(clamp(15, 0, 10)); print(max(2.5, 1));"
        ),
        "(0.0, 0.0, 1.0)\n0.0\n5.0\n(0.5, 0.5, 0.0)\n10\n2.5\n"
    );
}

#[test]
fn builtin_constants() {
    assert_eq!(
        output("print(PI); print(TAU / 2 == PI); print(log(E)); print(cos(PI));"),
        "3.141592653589793\ntrue\n1.0\n-1.0\n"
    );
    // A local may shadow a built-in constant.
    assert_eq!(output("let PI = 3; print(PI);"), "3\n");
}

#[test]
fn generic_functions() {
    let src = "fn mix<T: real | vector>(a: T, b: T, t: real) -> T { return a + (b - a) * t; }
               fn half<T: real | complex>(x: T) -> T { return x / 2; }
               fn fact<T: int | real>(n: T) -> T { if n <= 1 { return 1; } return n * fact(n - 1); }
               fn main() {
                   print(mix(0, 10, 0.25));
                   print(mix(vec2(0, 0), vec2(2, 4), 0.5));
                   print(half(3.0));
                   print(half(3.0 + 1.0i));
                   print(fact(10));
                   print(fact(10.0));
               }";
    assert_eq!(
        run(src).unwrap(),
        "2.5\n(1.0, 2.0)\n1.5\n1.5+0.5i\n3628800\n3628800.0\n"
    );
}

#[test]
fn for_counts_up_and_excludes_the_end() {
    assert_eq!(output("for i in 0..3 { print(i); }"), "0\n1\n2\n");
    assert_eq!(output("for i in -2..0 { print(i); }"), "-2\n-1\n");
}

#[test]
fn for_over_an_empty_range_does_not_run() {
    assert_eq!(output("for i in 3..3 { print(i); }"), "");
    assert_eq!(output("for i in 5..3 { print(i); }"), "");
}

#[test]
fn for_continue_still_advances() {
    assert_eq!(
        output("for i in 0..4 { if i == 1 { continue; } print(i); }"),
        "0\n2\n3\n"
    );
}

#[test]
fn for_break_stops_only_the_inner_loop() {
    assert_eq!(
        output("for i in 0..2 { for j in 0..5 { if j == 1 { break; } print(i * 10 + j); } }"),
        "0\n10\n"
    );
}

#[test]
fn for_return_leaves_the_function() {
    let src = "fn find() -> int {
                   for i in 0..10 { if i * i > 20 { return i; } }
                   return 0;
               }
               fn main() { let x = 1; print(find()); print(x); }";
    assert_eq!(run(src).unwrap(), "5\n1\n");
}

#[test]
fn for_bounds_are_evaluated_once_in_order() {
    assert_eq!(
        output("let n = 3; for i in 0..n { n = 1; print(i); } print(n);"),
        "0\n1\n2\n1\n"
    );
    let src = "fn a() -> int { print(1); return 0; }
               fn b() -> int { print(2); return 2; }
               fn main() { for i in a()..b() { print(10); } }";
    assert_eq!(run(src).unwrap(), "1\n2\n10\n10\n");
}

#[test]
fn for_variable_shadows_and_ends_with_the_loop() {
    assert_eq!(
        output("let i = 7; for i in 0..1 { print(i); } print(i);"),
        "0\n7\n"
    );
    assert_eq!(
        output("for i in 0..2 { let i = 5; i = i + 1; print(i); }"),
        "6\n6\n"
    );
}

#[test]
fn for_body_locals_are_fresh_each_iteration() {
    assert_eq!(
        output("for i in 0..2 { let x; if i == 0 { x = 1; } else { x = 2; } print(x); }"),
        "1\n2\n"
    );
}

#[test]
fn for_at_the_end_of_the_int_range_does_not_overflow() {
    assert_eq!(
        output("for i in 9223372036854775806..9223372036854775807 { print(i); }"),
        "9223372036854775806\n"
    );
}

#[test]
fn for_literals_and_generic_calls_are_rewritten() {
    assert_eq!(
        output("for i in 0..1 { let x: real = 1; print(x / 2); }"),
        "0.5\n"
    );
    let src = "fn id<T: int | real>(x: T) -> T { return x; }
               fn main() { for i in id(0)..id(2) { print(id(i)); } }";
    assert_eq!(run(src).unwrap(), "0\n1\n");
}

#[test]
fn runtime_error_in_a_for_bound() {
    assert_eq!(
        runtime_error("fn main() { let z = 0; for i in 0..1 / z { } }"),
        "division by zero"
    );
}

#[test]
fn print_arrays() {
    assert_eq!(output("print([1, 2]);"), "[1, 2]\n");
    assert_eq!(output("print([1.5, 2.0]);"), "[1.5, 2.0]\n");
    assert_eq!(output("print([[1], [2, 3]]);"), "[[1], [2, 3]]\n");
    assert_eq!(output("let a: [int] = []; print(a);"), "[]\n");
    assert_eq!(output("print([vec2(1, 2)]);"), "[(1.0, 2.0)]\n");
}

#[test]
fn array_literals_get_their_inferred_type() {
    assert_eq!(output("let a: [real] = [1, 2]; print(a);"), "[1.0, 2.0]\n");
    assert_eq!(
        output("let a: [real] = [0; 2]; a[0] = 1; print(a);"),
        "[1.0, 0.0]\n"
    );
    assert_eq!(output("let a: [complex] = [1]; print(a);"), "[1.0+0.0i]\n");
}

#[test]
fn array_fill_and_len() {
    assert_eq!(output("print([7; 3]);"), "[7, 7, 7]\n");
    assert_eq!(output("let n = 0; print(len([1.0; n]));"), "0\n");
    assert_eq!(output("print(len([[1], [2]]));"), "2\n");
}

#[test]
fn reading_and_writing_elements() {
    assert_eq!(output("let a = [1, 2, 3]; print(a[2]);"), "3\n");
    assert_eq!(output("print([1, 2][1]);"), "2\n");
    assert_eq!(
        output("let a = [1, 2]; a[0] = a[1] + 5; print(a);"),
        "[7, 2]\n"
    );
    assert_eq!(
        output("let m = [[1, 2], [3, 4]]; m[1][0] = 9; print(m); print(m[1][0]);"),
        "[[1, 2], [9, 4]]\n9\n"
    );
    assert_eq!(
        output("let m = [[1], [2]]; m[0] = [5, 6]; print(m);"),
        "[[5, 6], [2]]\n"
    );
}

#[test]
fn arrays_are_values() {
    assert_eq!(
        output("let a = [1, 2]; let b = a; b[0] = 9; print(a); print(b);"),
        "[1, 2]\n[9, 2]\n"
    );
    assert_eq!(
        output("let m = [[0; 2]; 2]; m[0][0] = 1; print(m);"),
        "[[1, 0], [0, 0]]\n"
    );
    assert_eq!(
        output("let m = [[1], [2]]; let row = m[0]; row[0] = 9; print(m); print(row);"),
        "[[1], [2]]\n[9]\n"
    );
    let src = "fn change(a: [int]) -> [int] { a[0] = 9; return a; }
               fn main() { let a = [1]; let b = change(a); print(a); print(b); }";
    assert_eq!(run(src).unwrap(), "[1]\n[9]\n");
}

#[test]
fn array_equality() {
    assert_eq!(output("print([1, 2] == [1, 2]);"), "true\n");
    assert_eq!(output("print([1, 2] != [1, 3]);"), "true\n");
    assert_eq!(output("print([1] == [1, 2]);"), "false\n");
}

#[test]
fn index_out_of_bounds() {
    let message =
        |i: &str, len: usize| format!("index {i} is out of bounds for an array of length {len}");
    assert_eq!(
        runtime_error("fn main() { let a = [1, 2]; print(a[2]); }"),
        message("2", 2)
    );
    assert_eq!(
        runtime_error("fn main() { let a = [1, 2]; print(a[-1]); }"),
        message("-1", 2)
    );
    assert_eq!(
        runtime_error("fn main() { let a = [1, 2]; a[5] = 1; }"),
        message("5", 2)
    );
    assert_eq!(
        runtime_error("fn main() { let m = [[1]]; m[0][1] = 1; }"),
        message("1", 1)
    );
    assert_eq!(
        runtime_error("fn main() { let a: [int] = []; print(a[0]); }"),
        message("0", 0)
    );
}

#[test]
fn bad_array_lengths() {
    assert_eq!(
        runtime_error("fn main() { let n = -1; print([0; n]); }"),
        "array length cannot be negative, got -1"
    );
    assert_eq!(
        runtime_error("fn main() { print([0; 9223372036854775807]); }"),
        "an array of 9223372036854775807 elements is too large"
    );
}

#[test]
fn index_assignment_evaluates_the_value_first() {
    let src = "fn v() -> int { print(1); return 5; }
               fn i() -> int { print(2); return 0; }
               fn main() { let a = [0]; a[i()] = v(); print(a); }";
    assert_eq!(run(src).unwrap(), "1\n2\n[5]\n");
}

#[test]
fn for_each_visits_elements_in_order() {
    assert_eq!(output("for x in [1, 2, 3] { print(x); }"), "1\n2\n3\n");
    let empty: &str = "let a: [int] = []; for x in a { print(x); }";
    assert_eq!(output(empty), "");
    assert_eq!(
        output("for row in [[1, 2], [3]] { for x in row { print(x); } }"),
        "1\n2\n3\n"
    );
}

#[test]
fn for_each_control_flow() {
    assert_eq!(
        output("for x in [1, 2, 3, 4] { if x == 2 { continue; } if x == 4 { break; } print(x); }"),
        "1\n3\n"
    );
    let src = "fn find(a: [int]) -> int {
                   for x in a { if x > 1 { return x; } }
                   return 0;
               }
               fn main() { print(find([1, 2, 3])); }";
    assert_eq!(run(src).unwrap(), "2\n");
}

#[test]
fn for_each_uses_the_array_from_the_start_of_the_loop() {
    assert_eq!(
        output("let a = [1, 2]; for x in a { a[1] = 9; print(x); } print(a);"),
        "1\n2\n[1, 9]\n"
    );
    assert_eq!(
        output("let a = [1, 2]; for x in a { a = [5]; print(x); } print(a);"),
        "1\n2\n[5]\n"
    );
}

#[test]
fn constant_arrays() {
    let src = "const A: [real] = [1, 2];
               fn main() { let b = A; b[0] = 5; print(A); print(b); print(A[1]); }";
    assert_eq!(run(src).unwrap(), "[1.0, 2.0]\n[5.0, 2.0]\n2.0\n");
}

#[test]
fn arrays_of_a_generic_parameter() {
    let src = "fn first<T: int | real>(a: [T]) -> T { return a[0]; }
               fn main() { print(first([1, 2])); print(first([1.5])); }";
    assert_eq!(run(src).unwrap(), "1\n1.5\n");
}

#[test]
fn generic_calls_and_literals_inside_arrays() {
    let src = "fn id<T: int | real>(x: T) -> T { return x; }
               fn main() {
                   let a: [real] = [id(1.5); 1 + id(1)];
                   a[id(0)] = 2;
                   print(a);
                   print([id(3)][id(0)]);
                   for x in [id(4)] { print(x); }
               }";
    assert_eq!(run(src).unwrap(), "[2.0, 1.5]\n3\n4\n");
}

#[test]
fn constants_use_built_ins() {
    assert_eq!(
        run("const A: real = sqrt(4.0); const V: [real] = [1.0, 2.0] * A; fn main() { print(V); }")
            .unwrap(),
        "[2.0, 4.0]\n"
    );
}

#[test]
fn compound_assignment() {
    assert_eq!(
        output("let x = 1; x += 2; x *= 5; x -= 3; x /= 2; print(x);"),
        "6\n"
    );
    assert_eq!(output("let x = 1.0; x /= 4; print(x);"), "0.25\n");
    assert_eq!(
        output("let v = vec2(1, 2); v *= 2.0; print(v);"),
        "(2.0, 4.0)\n"
    );
    assert_eq!(output("let a = [1, 2]; a[1] += 5; print(a);"), "[1, 7]\n");
    assert_eq!(
        output("let m = [[1, 2]]; m[0][1] *= 3; print(m);"),
        "[[1, 6]]\n"
    );
    assert_eq!(
        output("let a = [1]; let b = a; b[0] += 1; print(a); print(b);"),
        "[1]\n[2]\n"
    );
}

#[test]
fn compound_index_assignment_runs_each_part_once() {
    let src = "fn value() -> int { print(1); return 1; }
               fn index() -> int { print(2); return 0; }
               fn main() { let a = [1]; a[index()] += value(); print(a); }";
    assert_eq!(run(src).unwrap(), "1\n2\n[2]\n");
}

#[test]
fn compound_assignment_errors() {
    let run_body = |body: &str| runtime_error(&format!("fn main() {{ {body} }}"));
    assert_eq!(
        run_body("let x = 9223372036854775807; x += 1;"),
        "integer overflow"
    );
    assert_eq!(run_body("let x = 1; x /= 0;"), "division by zero");
    assert_eq!(
        run_body("let a = [1]; a[5] += 1;"),
        "index 5 is out of bounds for an array of length 1"
    );
}

#[test]
fn inclusive_ranges() {
    assert_eq!(output("for i in 1..=3 { print(i); }"), "1\n2\n3\n");
    assert_eq!(output("for i in 3..=3 { print(i); }"), "3\n");
    assert_eq!(output("for i in 4..=3 { print(i); }"), "");
}

#[test]
fn range_steps() {
    assert_eq!(output("for i in 0..7 step 3 { print(i); }"), "0\n3\n6\n");
    assert_eq!(output("for i in 0..6 step 3 { print(i); }"), "0\n3\n");
    assert_eq!(output("for i in 0..=6 step 3 { print(i); }"), "0\n3\n6\n");
    assert_eq!(output("for i in 3..0 step -1 { print(i); }"), "3\n2\n1\n");
    assert_eq!(output("for i in 3..=0 step -2 { print(i); }"), "3\n1\n");
    assert_eq!(output("for i in 0..3 step -1 { print(i); }"), "");
    assert_eq!(
        runtime_error("fn main() { for i in 0..3 step 0 { } }"),
        "loop step cannot be zero"
    );
}

#[test]
fn ranges_stop_at_the_end_of_int() {
    assert_eq!(
        output("for i in 9223372036854775806..=9223372036854775807 { print(i); }"),
        "9223372036854775806\n9223372036854775807\n"
    );
    assert_eq!(
        output("for i in 9223372036854775807..=9223372036854775807 step 5 { print(i); }"),
        "9223372036854775807\n"
    );
}

#[test]
fn range_parts_run_once_in_order() {
    let src = "fn n(x: int) -> int { print(x); return x; }
               fn main() { for i in n(0)..n(4) step n(2) { print(100 + i); } }";
    assert_eq!(run(src).unwrap(), "0\n4\n2\n100\n102\n");
}

#[test]
fn slices() {
    let body = |slice: &str| output(&format!("let a = [1, 2, 3]; print(a[{slice}]);"));
    assert_eq!(body("1..3"), "[2, 3]\n");
    assert_eq!(body("..2"), "[1, 2]\n");
    assert_eq!(body("1.."), "[2, 3]\n");
    assert_eq!(body(".."), "[1, 2, 3]\n");
    assert_eq!(body("0..=1"), "[1, 2]\n");
    assert_eq!(body("..=0"), "[1]\n");
    assert_eq!(body("1..1"), "[]\n");
    assert_eq!(body("3.."), "[]\n");
    assert_eq!(body("1..=2"), "[2, 3]\n");
}

#[test]
fn a_slice_is_a_copy() {
    assert_eq!(
        output("let a = [1, 2]; let b = a[..]; b[0] = 9; print(a); print(b);"),
        "[1, 2]\n[9, 2]\n"
    );
}

#[test]
fn slice_errors() {
    let error = |slice: &str| {
        runtime_error(&format!(
            "fn main() {{ let a = [1, 2, 3]; print(a[{slice}]); }}"
        ))
    };
    let of_three = "is out of bounds for an array of length 3";
    assert_eq!(error("2..1"), format!("slice 2..1 {of_three}"));
    assert_eq!(error("..4"), format!("slice 0..4 {of_three}"));
    assert_eq!(error("4.."), format!("slice 4..3 {of_three}"));
    assert_eq!(error("-1.."), format!("slice -1..3 {of_three}"));
    assert_eq!(error("..=3"), format!("slice 0..=3 {of_three}"));
    assert_eq!(error("0..=-1"), format!("slice 0..=-1 {of_three}"));
    assert_eq!(
        error("..=9223372036854775807"),
        format!("slice 0..=9223372036854775807 {of_three}")
    );
}

#[test]
fn push_and_concat_return_new_arrays() {
    assert_eq!(
        output("let a = [1]; let b = push(a, 2); print(a); print(b);"),
        "[1]\n[1, 2]\n"
    );
    assert_eq!(
        output("let a: [int] = []; a = push(a, 1); a = push(a, 2); print(a);"),
        "[1, 2]\n"
    );
    assert_eq!(output("print(concat([1], [2, 3]));"), "[1, 2, 3]\n");
    assert_eq!(
        output("let m = [[1]]; m = push(m, [2]); print(m);"),
        "[[1], [2]]\n"
    );
}

#[test]
fn element_wise_operators() {
    assert_eq!(
        output("print([1, 2] + [3, 4]); print([1.5] - [0.5]);"),
        "[4, 6]\n[1.0]\n"
    );
    assert_eq!(output("print(-[1, 2]);"), "[-1, -2]\n");
    assert_eq!(output("print([[1, 2]] + [[3, 4]]);"), "[[4, 6]]\n");
    assert_eq!(
        output("print([1, 2] * 2); print(2 * [1, 2]); print([4.0] / 2);"),
        "[2, 4]\n[2, 4]\n[2.0]\n"
    );
    assert_eq!(output("print([[1, 2]] * 3);"), "[[3, 6]]\n");
    assert_eq!(output("print([1, 2] * 2.5);"), "[2.5, 5.0]\n");
    assert_eq!(output("print([vec2(1, 2)] * 2.0);"), "[(2.0, 4.0)]\n");
    assert_eq!(output("let a = [1]; a += [2]; a *= 3; print(a);"), "[9]\n");
}

#[test]
fn element_wise_errors() {
    let error = |body: &str| runtime_error(&format!("fn main() {{ {body} }}"));
    assert_eq!(
        error("print([1] + [1, 2]);"),
        "arrays have different lengths: 1 and 2"
    );
    assert_eq!(
        error("print([[1]] + [[1, 2]]);"),
        "arrays have different lengths: 1 and 2"
    );
    assert_eq!(error("print([1] / 0);"), "division by zero");
    assert_eq!(
        error("print([9223372036854775807] + [1]);"),
        "integer overflow"
    );
}

#[test]
fn generic_functions_on_arrays() {
    let src = "fn twice<T: real | vec2>(a: [T]) -> [T] { return a + a; }
               fn main() { print(twice([1.5])); print(twice([vec2(1, 2)])); }";
    assert_eq!(run(src).unwrap(), "[3.0]\n[(2.0, 4.0)]\n");
}

#[test]
fn matrix_product() {
    assert_eq!(
        output("print([[1, 2], [3, 4]] * [[5, 6], [7, 8]]);"),
        "[[19, 22], [43, 50]]\n"
    );
    assert_eq!(output("print([[1, 2], [3, 4]] * [1, 1]);"), "[3, 7]\n");
    assert_eq!(
        output("print([[1, 2, 3], [4, 5, 6]] * [[1], [1], [1]]);"),
        "[[6], [15]]\n"
    );
    assert_eq!(output("print([[1.0i]] * [[1.0i]]);"), "[[-1.0+0.0i]]\n");
    assert_eq!(output("let m = [[2]]; m *= m; print(m);"), "[[4]]\n");
    // The right side is untouched by the product.
    assert_eq!(
        output("let m = [[2]]; let n = m * m; n[0][0] = 9; print(m);"),
        "[[2]]\n"
    );
}

#[test]
fn matrix_product_errors() {
    let error = |body: &str| runtime_error(&format!("fn main() {{ {body} }}"));
    assert_eq!(
        error("print([[1, 2]] * [[1, 2]]);"),
        "cannot multiply a 1x2 matrix by a 1x2 matrix"
    );
    assert_eq!(
        error("print([[1, 2]] * [1]);"),
        "cannot multiply a 1x2 matrix by a vector of length 1"
    );
    assert_eq!(
        error("print([[1, 2], [3]] * [1, 2]);"),
        "matrix is not rectangular: row 1 has 1 elements, row 0 has 2"
    );
    assert_eq!(
        error("print([[1]] * [[1], [2, 3]]);"),
        "matrix is not rectangular: row 1 has 2 elements, row 0 has 1"
    );
    assert_eq!(
        error("let m: [[int]] = [[1]]; let v: [int] = []; print(m * v);"),
        "cannot multiply a 1x1 matrix by an empty array"
    );
}

#[test]
fn matrix_product_with_an_empty_left_side() {
    assert_eq!(
        output("let m: [[int]] = []; let v: [int] = [1]; print(m * v);"),
        "[]\n"
    );
}

#[test]
fn tuples_print_and_compare() {
    assert_eq!(output("print((1, 2.5));"), "(1, 2.5)\n");
    assert_eq!(output("print((1, (true, [2])));"), "(1, (true, [2]))\n");
    assert_eq!(
        output("print((1, 2) == (1, 2)); print((1, 2) != (1, 2));"),
        "true\nfalse\n"
    );
}

#[test]
fn functions_return_tuples() {
    let src = "fn div_rem(a: int, b: int) -> (int, int) { return (a / b, a - a / b * b); }
               fn main() { let (q, r) = div_rem(17, 5); print(q); print(r); }";
    assert_eq!(run(src).unwrap(), "3\n2\n");
}

#[test]
fn unpacking_skips_underscores() {
    assert_eq!(output("let (_, b, _) = (1, 2, 3); print(b);"), "2\n");
}

#[test]
fn unpacked_variables_can_be_changed() {
    assert_eq!(
        output("let t = (1, 2); let (a, b) = t; a += 10; print(a); print(t);"),
        "11\n(1, 2)\n"
    );
}

#[test]
fn literals_inside_tuples_get_their_type() {
    assert_eq!(
        output("let t: (real, complex) = (1, 2); print(t);"),
        "(1.0, 2.0+0.0i)\n"
    );
    assert_eq!(
        output("let (a, b) = (1, 2.5); print(a); print(b);"),
        "1\n2.5\n"
    );
}

#[test]
fn tuples_and_arrays_nest() {
    assert_eq!(
        output(
            "let a = [(1, 2.0), (3, 4.0)]; a[0] = (9, 9.0); let (x, y) = a[1]; print(a); print(x); print(y);"
        ),
        "[(9, 9.0), (3, 4.0)]\n3\n4.0\n"
    );
}

#[test]
fn generic_tuples() {
    let src = "fn swap<T: int | real>(p: (T, T)) -> (T, T) { let (a, b) = p; return (b, a); }
               fn main() { print(swap((1, 2))); print(swap((1.5, 2))); }";
    assert_eq!(run(src).unwrap(), "(2, 1)\n(2.0, 1.5)\n");
}

#[test]
fn tuple_constants() {
    let src = "const P: (int, real) = (1, sqrt(4.0));
               fn main() { let (a, b) = P; print(a); print(b); }";
    assert_eq!(run(src).unwrap(), "1\n2.0\n");
}

#[test]
fn strings_print_without_quotes() {
    assert_eq!(output(r#"print("hi");"#), "hi\n");
    assert_eq!(output(r#"print("a\tb\n\"c\"\\");"#), "a\tb\n\"c\"\\\n");
    assert_eq!(output(r#"print("");"#), "\n");
}

#[test]
fn strings_join_and_compare() {
    assert_eq!(output(r#"print("a" + "b" + "c");"#), "abc\n");
    assert_eq!(output(r#"let s = "a"; s += "b"; print(s);"#), "ab\n");
    assert_eq!(
        output(r#"let a = "x"; let b = a; b += "y"; print(a); print(b);"#),
        "x\nxy\n"
    );
    assert_eq!(
        output(r#"print("a" == "a"); print("a" != "a");"#),
        "true\nfalse\n"
    );
    assert_eq!(output(r#"print(["a"] + ["b"]);"#), "[ab]\n");
}

#[test]
fn string_length_counts_characters() {
    assert_eq!(output(r#"print(len("")); print(len("abc"));"#), "0\n3\n");
    assert_eq!(output(r#"print(len("привет"));"#), "6\n");
    assert_eq!(output(r#"print(len("a\n"));"#), "2\n");
}

#[test]
fn string_conversion() {
    assert_eq!(
        output(r#"print(string(1) + "|" + string(1.5));"#),
        "1|1.5\n"
    );
    assert_eq!(
        output(r#"print(string(true) + "|" + string(2.0i));"#),
        "true|0.0+2.0i\n"
    );
    assert_eq!(output(r#"print(string(vec2(1, 2)));"#), "(1.0, 2.0)\n");
    assert_eq!(
        output(r#"print(string([1, 2]) + string((1, 2)));"#),
        "[1, 2](1, 2)\n"
    );
    assert_eq!(output(r#"print(string("x"));"#), "x\n");
}

#[test]
fn strings_in_generics_and_constants() {
    let src = r#"const GREETING: string = "hi" + "!";
                 fn twice<T: int | string>(x: T) -> T { return x + x; }
                 fn main() { print(twice(GREETING)); print(twice(2)); }"#;
    assert_eq!(run(src).unwrap(), "hi!hi!\n4\n");
}

#[test]
fn plot_writes_a_file() {
    let path = std::env::temp_dir().join(format!("kmpl_plot_{}.svg", std::process::id()));
    let path_text = path.to_str().unwrap();
    let src =
        format!(r#"fn main() {{ plot([0, 1, 2], [1, 3, 2], "{path_text}"); print("done"); }}"#);
    assert_eq!(run(&src).unwrap(), "done\n");

    let image = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(image.starts_with("<svg"), "{image}");
    assert!(image.contains("<polyline"), "{image}");
}

#[test]
fn plot_errors() {
    assert_eq!(
        runtime_error(r#"fn main() { plot([1.0, 2.0], [1.0], "x.svg"); }"#),
        "`plot` needs as many x values as y values, got 2 and 1"
    );
    let message = runtime_error(r#"fn main() { plot([1.0], [1.0], "/nonexistent-dir/x.svg"); }"#);
    assert!(
        message.starts_with("cannot write `/nonexistent-dir/x.svg`: "),
        "{message}"
    );
}

const POINT: &str = "struct P { x: real, n: int } ";

fn with_point(body: &str) -> String {
    run(&format!("{POINT} fn main() {{ {body} }}")).unwrap()
}

#[test]
fn struct_values() {
    assert_eq!(
        with_point("let p = P { x: 1, n: 2 }; print(p.x); print(p.n);"),
        "1.0\n2\n"
    );
    assert_eq!(
        with_point("print(P { n: 2, x: 1 });"),
        "P { x: 1.0, n: 2 }\n"
    );
    assert_eq!(
        run("struct Empty { } fn main() { print(Empty { }); }").unwrap(),
        "Empty { }\n"
    );
    assert_eq!(
        with_point(
            "print(P { x: 1, n: 2 } == P { n: 2, x: 1 }); print(P { x: 1, n: 2 } != P { x: 2, n: 2 });"
        ),
        "true\ntrue\n"
    );
    assert_eq!(
        with_point("print(string(P { x: 1, n: 2 }));"),
        "P { x: 1.0, n: 2 }\n"
    );
}

#[test]
fn struct_fields_run_in_written_order() {
    let src = "struct P { a: int, b: int }
               fn v(x: int) -> int { print(x); return x; }
               fn main() { print(P { b: v(2), a: v(1) }); }";
    assert_eq!(run(src).unwrap(), "2\n1\nP { a: 1, b: 2 }\n");
}

#[test]
fn structs_are_values() {
    assert_eq!(
        with_point("let a = P { x: 1, n: 1 }; let b = a; b.n = 2; print(a.n); print(b.n);"),
        "1\n2\n"
    );
    let src = "struct P { n: int }
               fn change(p: P) -> P { p.n = 9; return p; }
               fn main() { let a = P { n: 1 }; let b = change(a); print(a.n); print(b.n); }";
    assert_eq!(run(src).unwrap(), "1\n9\n");
    assert_eq!(
        with_point(
            "let a = [P { x: 1, n: 1 }]; let b = a; b[0].n = 5; print(a[0].n); print(b[0].n);"
        ),
        "1\n5\n"
    );
}

#[test]
fn nested_field_assignment() {
    let src = "struct In { v: [int] }
               struct Out { a: In, list: [In] }
               fn main() {
                   let o = Out { a: In { v: [1] }, list: [In { v: [2, 3] }] };
                   o.a.v[0] = 7;
                   o.list[0].v[1] += 10;
                   print(o.a.v);
                   print(o.list[0].v);
                   o.a = In { v: [] };
                   print(len(o.a.v));
               }";
    assert_eq!(run(src).unwrap(), "[7]\n[2, 13]\n0\n");
}

#[test]
fn compound_field_assignment_runs_each_part_once() {
    let src = "struct P { n: int }
               fn value() -> int { print(1); return 1; }
               fn index() -> int { print(2); return 0; }
               fn main() { let a = [P { n: 1 }]; a[index()].n += value(); print(a[0].n); }";
    assert_eq!(run(src).unwrap(), "1\n2\n2\n");
}

#[test]
fn field_paths_report_bad_indices() {
    assert_eq!(
        runtime_error(&format!(
            "{POINT} fn main() {{ let a = [P {{ x: 1, n: 1 }}]; a[3].n = 1; }}"
        )),
        "index 3 is out of bounds for an array of length 1"
    );
    assert_eq!(
        runtime_error(&format!(
            "{POINT} fn main() {{ let a = [P {{ x: 1, n: 1 }}]; a[3].n += 1; }}"
        )),
        "index 3 is out of bounds for an array of length 1"
    );
}

#[test]
fn field_reads_of_calls_and_elements() {
    let src = "struct P { n: int }
               fn make() -> P { return P { n: 4 }; }
               fn main() { print(make().n); print([make()][0].n); print((P { n: 5 }).n); }";
    assert_eq!(run(src).unwrap(), "4\n4\n5\n");
}

#[test]
fn literals_get_field_types() {
    assert_eq!(
        with_point("let p = P { x: 1, n: 2 }; print(p.x / 2);"),
        "0.5\n"
    );
    assert_eq!(
        with_point("let p = P { x: 1, n: 2 }; p.x += 1; print(p.x);"),
        "2.0\n"
    );
}

#[test]
fn struct_constants_and_recursive_data() {
    let src = "struct Node { value: int, children: [Node] }
               const LEAF: Node = Node { value: 1, children: [] };
               fn sum(n: Node) -> int {
                   let total = n.value;
                   for c in n.children { total += sum(c); }
                   return total;
               }
               fn main() { print(sum(Node { value: 2, children: [LEAF, LEAF] })); }";
    assert_eq!(run(src).unwrap(), "4\n");
}

#[test]
fn structs_in_if_headers_with_brackets() {
    assert_eq!(
        with_point(r#"let a = P { x: 1, n: 1 }; if a == (P { x: 1, n: 1 }) { print("same"); }"#),
        "same\n"
    );
}
