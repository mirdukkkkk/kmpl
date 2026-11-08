use kmpl_ast::{ExprKind, Item, StmtKind};

use super::{assert_error, assert_ok, errors};
use crate::check;

#[test]
fn unknown_type_names() {
    assert_error("fn f(x: foo) { }", "unknown type `foo`");
    assert_error("fn f() -> str { return 1; }", "unknown type `str`");
    assert_error("fn f() { let x: foo = 1; }", "unknown type `foo`");
    assert_error("const N: T = 1;", "unknown type `T`");
    // A generic name is only known inside its own function.
    assert_error("fn f<T>(x: T) { } fn g(y: T) { }", "unknown type `T`");
}

#[test]
fn generic_names_are_unique() {
    assert_error("fn f<T, T>() { }", "`T` is already defined");
}

#[test]
fn main_cannot_be_generic() {
    assert_eq!(
        errors("fn main<T>() { }"),
        vec!["`main` must not be generic"]
    );
}

#[test]
fn generic_function_with_several_types() {
    assert_ok(
        "fn mix<T: real | vector>(a: T, b: T, t: real) -> T { return a + (b - a) * t; }
         fn main() {
             let x: real = mix(0, 10, 0.5);
             let v: vec2 = mix(vec2(0, 0), vec2(2, 4), 0.25);
         }",
    );
}

#[test]
fn bound_too_wide_for_the_body() {
    // `(b - a) * t` is `complex * real`, which needs `complex(t, 0)`.
    assert_error(
        "fn mix<T: float | vector>(a: T, b: T, t: real) -> T { return a + (b - a) * t; }",
        "in `mix<complex>`: mismatched types: expected `complex`, found `real`",
    );
}

#[test]
fn body_is_checked_for_every_type_in_the_bound() {
    // Never called with `bool`, still an error.
    assert_error(
        "fn twice<T: int | bool>(x: T) -> T { return x + x; }",
        "in `twice<bool>`: cannot apply `+` to `bool`",
    );
    assert_error(
        "fn first<T: vector>(v: T) -> real { return v:z; }",
        "in `first<vec2>`: invalid swizzle `z` for `vec2`: use 1 to 4 of the letters `xy`",
    );
}

#[test]
fn an_error_that_does_not_depend_on_the_type_is_reported_once() {
    assert_eq!(
        errors("fn f<T: int | real>(x: T) { let b: bool = 1.5; }"),
        vec!["in `f<int>`: mismatched types: expected `bool`, found `real` or `complex`"]
    );
}

#[test]
fn let_annotations_use_the_generic_type() {
    assert_ok("fn f<T: number>(x: T) -> T { let y: T = x * x; return y; }");
    assert_error(
        "fn f<T: int | real>(x: T) { let y: int = x; }",
        "in `f<real>`: mismatched types: expected `int`, found `real`",
    );
}

#[test]
fn call_must_fit_the_bound() {
    assert_error(
        "fn g<T: vector>(v: T) { } fn main() { g(1.0); }",
        "mismatched types: expected `vec2`, `vec3` or `vec4`, found `real` or `complex`",
    );
    assert_error(
        "fn same<T>(a: T, b: T) { } fn main() { same(1.0, vec2(1, 2)); }",
        "mismatched types: expected `real` or `complex`, found `vec2`",
    );
}

#[test]
fn generic_type_can_come_from_the_result() {
    assert_ok(
        "fn zero<T: int | real>() -> T { return 0; }
         fn main() { let x: real = zero(); let n = zero(); print(n + 1); }",
    );
}

#[test]
fn generic_parameter_that_cannot_be_inferred() {
    assert_error(
        "fn ignore<T: vec2 | vec3>() { } fn main() { ignore(); }",
        "cannot infer the generic parameters of `ignore` here, add a type annotation",
    );
}

#[test]
fn too_many_copies() {
    assert_error(
        "fn f<A, B, C>() { }",
        "`f` would need 343 copies, more than 256; narrow the bounds of its generic parameters",
    );
}

#[test]
fn generic_calls_inside_generic_functions() {
    assert_ok(
        "fn twice<T: number>(x: T) -> T { return x * 2; }
         fn quad<T: number>(x: T) -> T { return twice(twice(x)); }
         fn fact<T: int | real>(n: T) -> T { if n <= 1 { return 1; } return n * fact(n - 1); }
         fn main() { let a: real = quad(1.5); let b: int = fact(5); }",
    );
}

/// After checking, generic originals are gone, copies exist, and calls
/// name their copy.
#[test]
fn program_is_monomorphized() {
    let src = "fn twice<T: int | real>(x: T) -> T { return x * 2; }
               fn main() { print(twice(1.5)); }";
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    check(&mut program).unwrap();

    let names: Vec<&str> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f.name.name.as_str()),
            Item::Const(_) => None,
        })
        .collect();
    assert_eq!(names, vec!["main", "twice<int>", "twice<real>"]);

    let Item::Function(main) = &program.items[0] else {
        panic!("expected main");
    };
    let StmtKind::Expr(print) = &main.body[0].kind else {
        panic!("expected a call");
    };
    let ExprKind::Call { args, .. } = &print.kind else {
        panic!("expected print");
    };
    let ExprKind::Call { name, .. } = &args[0].kind else {
        panic!("expected twice");
    };
    assert_eq!(name.name, "twice<real>");
}

#[test]
fn literals_get_their_type_per_copy() {
    let src = "fn half<T: real | complex>(x: T) -> T { return x / 2; }
               fn main() { }";
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    check(&mut program).unwrap();

    let divisor = |item: &Item| {
        let Item::Function(f) = item else {
            panic!("expected a function");
        };
        let StmtKind::Return(Some(e)) = &f.body[0].kind else {
            panic!("expected return");
        };
        let ExprKind::Binary { rhs, .. } = &e.kind else {
            panic!("expected division");
        };
        rhs.kind.clone()
    };
    assert_eq!(divisor(&program.items[1]), ExprKind::Real(2.0));
    assert_eq!(
        divisor(&program.items[2]),
        ExprKind::Complex { re: 2.0, im: 0.0 }
    );
}

#[test]
fn generic_annotations_inside_for_are_substituted() {
    assert_ok("fn f<T: real | vec2>(x: T) { for i in 0..2 { let y: T = x; print(y); } }");
}

#[test]
fn arrays_of_a_generic_parameter() {
    assert_ok(
        "fn first<T: int | real>(a: [T]) -> T { return a[0]; }
         fn f() { let x: real = first([1.5]); let y: int = first([1]); }",
    );
    assert_ok("fn f<T: real | vec2>(x: T) { let a: [T] = [x; 2]; print(a[0]); }");
    assert_error(
        "fn first<T: int | real>(a: [T]) -> T { return a[0]; }
         fn f() { first([true]); }",
        "mismatched types: expected `[int]`, found `[bool]`",
    );
}

#[test]
fn a_generic_parameter_cannot_be_an_array() {
    assert_error(
        "fn g<T: int>(x: T) { } fn f() { g([1]); }",
        "mismatched types: expected `int`, found `[int]`",
    );
}

#[test]
fn tuples_of_generic_parameters() {
    assert_ok(
        "fn first<T: int | real>(p: (T, T)) -> T { let (a, _) = p; return a; } fn g() { let x: real = first((1.5, 2)); }",
    );
    assert_error(
        "fn id<T>(x: T) -> T { return x; } fn g() { id((1, 2)); }",
        "mismatched types: expected `int`, `real`, `complex`, `bool`, `vec2`, `vec3` or `vec4`, found `(int, int)`",
    );
}
