use kmpl_ast::{Item, Span, Ty, Type};

use crate::check;

fn parse(src: &str) -> kmpl_ast::Program {
    kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap()
}

fn span_of(src: &str, part: &str) -> Span {
    let start = src
        .find(part)
        .unwrap_or_else(|| panic!("no `{part}` in `{src}`"));
    Span::new(start, start + part.len())
}

/// The type of the expression `part` in `fn main`.
fn type_in_main(src: &str, part: &str) -> Option<Ty> {
    let mut program = parse(src);
    let info = check(&mut program).unwrap();
    let main = program
        .items
        .iter()
        .position(|item| matches!(item, Item::Function(f) if f.name.name == "main"))
        .unwrap();
    info.of(main, span_of(src, part)).cloned()
}

const REAL: Ty = Ty::Prim(Type::Real);
const INT: Ty = Ty::Prim(Type::Int);

#[test]
fn numbers_after_inference() {
    let src = "fn main() { let a: real = 1; let b = 2; print(a + 3); print(b); }";
    assert_eq!(type_in_main(src, "1"), Some(REAL));
    assert_eq!(type_in_main(src, "2"), Some(INT));
    assert_eq!(type_in_main(src, "3"), Some(REAL));
    assert_eq!(type_in_main(src, "a + 3"), Some(REAL));
}

#[test]
fn compound_types() {
    let src = "struct P { x: real }
               fn main() { let a = [1, 2.5]; let t = (1, a); let p = P { x: 0 }; print(p.x); print(t); }";
    let array = Ty::Array(Box::new(REAL));
    assert_eq!(type_in_main(src, "[1, 2.5]"), Some(array.clone()));
    assert_eq!(
        type_in_main(src, "(1, a)"),
        Some(Ty::Tuple(vec![INT, array]))
    );
    assert_eq!(
        type_in_main(src, "P { x: 0 }"),
        Some(Ty::Struct("P".to_owned()))
    );
    assert_eq!(type_in_main(src, "p.x"), Some(REAL));
}

#[test]
fn results_of_calls_and_indexing() {
    let src = "fn f() -> [int] { return [1]; } fn main() { print(f()[0]); print(sqrt(4.0)); }";
    let mut program = parse(src);
    let info = check(&mut program).unwrap();
    let call = src.rfind("f()").unwrap();
    assert_eq!(
        info.of(1, Span::new(call, call + 3)),
        Some(&Ty::Array(Box::new(INT)))
    );
    assert_eq!(type_in_main(src, "f()[0]"), Some(INT));
    assert_eq!(type_in_main(src, "sqrt(4.0)"), Some(REAL));
}

#[test]
fn indices_follow_removed_generic_functions() {
    let src =
        "fn id<T: int | real>(x: T) -> T { return x; } fn main() { let a = id(1.5); print(a); }";
    assert_eq!(type_in_main(src, "id(1.5)"), Some(REAL));
}

#[test]
fn each_copy_of_a_generic_function_has_its_own_types() {
    let src =
        "fn id<T: int | real>(x: T) -> T { return x; } fn main() { print(id(1)); print(id(1.5)); }";
    let mut program = parse(src);
    let info = check(&mut program).unwrap();
    let span = span_of(src, "x; }");
    let span = Span::new(span.start, span.start + 1);

    let index_of = |name: &str| {
        program
            .items
            .iter()
            .position(|item| matches!(item, Item::Function(f) if f.name.name == name))
            .unwrap()
    };
    assert_eq!(info.of(index_of("id<int>"), span), Some(&INT));
    assert_eq!(info.of(index_of("id<real>"), span), Some(&REAL));
}
