//! Test helpers. Trees are printed as short S-expressions so tests
//! show the shape of the AST: `(+ 1 (* 2 3))`, `(let x real 1.0)`.

mod expr;
mod item;
mod stmt;

use kmpl_ast::{BinOp, Block, Expr, ExprKind, Item, Program, Stmt, StmtKind, Type, UnOp};

use crate::ParseError;

fn lex(src: &str) -> Vec<(kmpl_lexer::Token, kmpl_ast::Span)> {
    kmpl_lexer::lex(src).unwrap()
}

fn parse_expr(src: &str) -> Result<Expr, ParseError> {
    crate::parse_expr(lex(src))
}

fn parse_program(src: &str) -> Result<Program, ParseError> {
    crate::parse(lex(src))
}

/// Parses `src` as the body of `fn main() { ... }` and prints its statements.
fn body(src: &str) -> String {
    let program = parse_program(&format!("fn main() {{ {src} }}")).unwrap();
    match &program.items[..] {
        [Item::Function(f)] => sexp_block(&f.body),
        _ => panic!("expected one function"),
    }
}

/// Error message from parsing `src` as a function body.
fn body_error(src: &str) -> String {
    parse_program(&format!("fn main() {{ {src} }}"))
        .unwrap_err()
        .message
}

fn sexp_program(p: &Program) -> String {
    let items: Vec<String> = p.items.iter().map(sexp_item).collect();
    items.join(" ")
}

fn sexp_item(item: &Item) -> String {
    match item {
        Item::Const(c) => format!(
            "(const {} {} {})",
            c.name.name,
            sexp_ty(c.ty),
            sexp(&c.value)
        ),
        Item::Function(f) => {
            let params: Vec<String> = f
                .params
                .iter()
                .map(|p| format!("({} {})", p.name.name, sexp_ty(p.ty)))
                .collect();
            let ret = f.ret.map_or("_".to_owned(), sexp_ty);
            format!(
                "(fn {} [{}] {} {})",
                f.name.name,
                params.join(" "),
                ret,
                sexp_block(&f.body)
            )
        }
    }
}

fn sexp_block(block: &Block) -> String {
    let stmts: Vec<String> = block.iter().map(sexp_stmt).collect();
    format!("[{}]", stmts.join(" "))
}

fn sexp_stmt(s: &Stmt) -> String {
    match &s.kind {
        StmtKind::Let { name, ty, init } => {
            let ty = ty.map_or("_".to_owned(), sexp_ty);
            let init = init.as_ref().map_or("_".to_owned(), sexp);
            format!("(let {} {ty} {init})", name.name)
        }
        StmtKind::Assign { name, value } => format!("(= {} {})", name.name, sexp(value)),
        StmtKind::If {
            cond,
            then,
            otherwise,
        } => match otherwise {
            Some(other) => format!(
                "(if {} {} {})",
                sexp(cond),
                sexp_block(then),
                sexp_block(other)
            ),
            None => format!("(if {} {})", sexp(cond), sexp_block(then)),
        },
        StmtKind::While { cond, body } => format!("(while {} {})", sexp(cond), sexp_block(body)),
        StmtKind::Break => "break".to_owned(),
        StmtKind::Continue => "continue".to_owned(),
        StmtKind::Return(Some(e)) => format!("(return {})", sexp(e)),
        StmtKind::Return(None) => "(return)".to_owned(),
        StmtKind::Expr(e) => sexp(e),
    }
}

fn sexp_ty(ty: Type) -> String {
    format!("{ty:?}").to_lowercase()
}

fn sexp(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(v) => v.to_string(),
        ExprKind::Real(v) => format!("{v:?}"),
        ExprKind::Imag(v) => format!("{v:?}i"),
        ExprKind::Bool(v) => v.to_string(),
        ExprKind::Var(id) => id.name.clone(),
        ExprKind::Unary { op, operand } => {
            let op = match op {
                UnOp::Neg => "-",
                UnOp::Not => "!",
            };
            format!("({op} {})", sexp(operand))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let op = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
            };
            format!("({op} {} {})", sexp(lhs), sexp(rhs))
        }
        ExprKind::Call { name, args } => {
            let args: Vec<String> = args.iter().map(sexp).collect();
            format!("(call {} [{}])", name.name, args.join(" "))
        }
        ExprKind::Swizzle { base, mask } => format!("(: {} {})", sexp(base), mask.name),
    }
}
