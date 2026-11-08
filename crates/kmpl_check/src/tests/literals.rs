use kmpl_ast::{Expr, ExprKind, Item, Program, StmtKind};

use crate::check;

/// Parses and checks `src`, returning the program after literal rewriting.
fn checked(src: &str) -> Program {
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    check(&mut program).unwrap();
    program
}

/// Initializers of the `let` statements in `fn main() { body }`.
fn let_inits(body: &str) -> Vec<ExprKind> {
    let program = checked(&format!("fn main() {{ {body} }}"));
    let Item::Function(main) = &program.items[0] else {
        panic!("expected main");
    };
    main.body
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::Let { init: Some(e), .. } => Some(e.kind.clone()),
            _ => None,
        })
        .collect()
}

fn one(body: &str) -> ExprKind {
    let mut inits = let_inits(body);
    assert_eq!(inits.len(), 1);
    inits.remove(0)
}

fn complex(re: f64, im: f64) -> ExprKind {
    ExprKind::Complex { re, im }
}

fn kinds(exprs: &[Expr]) -> Vec<ExprKind> {
    exprs.iter().map(|e| e.kind.clone()).collect()
}

#[test]
fn defaults_keep_literals() {
    assert_eq!(one("let x = 1;"), ExprKind::Int(1));
    assert_eq!(one("let x = 1.5;"), ExprKind::Real(1.5));
    assert_eq!(one("let x = 2.0i;"), complex(0.0, 2.0));
}

#[test]
fn int_literal_used_as_real_or_complex() {
    assert_eq!(one("let x: real = 1;"), ExprKind::Real(1.0));
    assert_eq!(one("let x: complex = 3;"), complex(3.0, 0.0));
}

#[test]
fn real_literal_used_as_complex() {
    assert_eq!(one("let x: complex = 2.5;"), complex(2.5, 0.0));
}

#[test]
fn literals_inside_expressions() {
    let ExprKind::Unary { operand, .. } = one("let z: complex = -1;") else {
        panic!("expected unary minus");
    };
    assert_eq!(operand.kind, complex(1.0, 0.0));

    let ExprKind::Binary { lhs, rhs, .. } = one("let z = 2.0 + 3.0i;") else {
        panic!("expected addition");
    };
    assert_eq!(lhs.kind, complex(2.0, 0.0));
    assert_eq!(rhs.kind, complex(0.0, 3.0));
}

#[test]
fn builtin_arguments() {
    let ExprKind::Call { args, .. } = one("let v = vec3(1, 2, 3);") else {
        panic!("expected call");
    };
    assert_eq!(
        kinds(&args),
        vec![
            ExprKind::Real(1.0),
            ExprKind::Real(2.0),
            ExprKind::Real(3.0)
        ]
    );
}

#[test]
fn type_flows_back_to_earlier_literal() {
    let inits = let_inits("let a = 1; let b: real = a;");
    assert_eq!(inits[0], ExprKind::Real(1.0));
}

#[test]
fn constants_are_rewritten() {
    let program = checked("const N: real = 2; fn main() { }");
    let Item::Const(n) = &program.items[0] else {
        panic!("expected const");
    };
    assert_eq!(n.value.kind, ExprKind::Real(2.0));
}

#[test]
fn nothing_is_rewritten_when_check_fails() {
    let src = "fn main() { let x: real = 1; let y: bool = 2; }";
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    assert!(check(&mut program).is_err());

    let Item::Function(main) = &program.items[0] else {
        panic!("expected main");
    };
    let StmtKind::Let { init: Some(e), .. } = &main.body[0].kind else {
        panic!("expected let");
    };
    assert_eq!(e.kind, ExprKind::Int(1));
}
