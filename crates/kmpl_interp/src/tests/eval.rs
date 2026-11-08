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
