//! Test helpers. Trees are printed as short S-expressions so tests
//! show the shape of the AST: `(+ 1 (* 2 3))`, `(let x real 1.0)`.

mod expr;
mod item;
mod stmt;

use kmpl_ast::{
    BinOp, Block, Expr, ExprKind, Item, PathStep, Program, Stmt, StmtKind, Type, TypeExpr, UnOp,
};

use kmpl_ast::Pattern;

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
            sexp_ty(&c.ty),
            sexp(&c.value)
        ),
        Item::Enum(e) => {
            let variants: Vec<String> = e
                .variants
                .iter()
                .map(|v| {
                    let fields: Vec<String> = v.fields.iter().map(sexp_ty).collect();
                    format!("({} {})", v.name.name, fields.join(" "))
                })
                .collect();
            format!("(enum {} [{}])", e.name.name, variants.join(" "))
        }
        Item::Struct(s) => {
            let fields: Vec<String> = s
                .fields
                .iter()
                .map(|f| format!("({} {})", f.name.name, sexp_ty(&f.ty)))
                .collect();
            format!("(struct {} [{}])", s.name.name, fields.join(" "))
        }
        Item::Function(f) => {
            let params: Vec<String> = f
                .params
                .iter()
                .map(|p| format!("({} {})", p.name.name, sexp_ty(&p.ty)))
                .collect();
            let ret = f.ret.as_ref().map_or("_".to_owned(), sexp_ty);
            format!(
                "(fn {}{} [{}] {} {})",
                f.name.name,
                sexp_generics(f),
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

fn arm_name(pattern: &Pattern) -> String {
    let bindings: Vec<&str> = pattern.bindings().iter().map(|n| n.name.as_str()).collect();
    let head = match pattern {
        Pattern::Variant { name, variant, .. } => format!("{}::{}", name.name, variant.name),
        Pattern::Some(_) => "some".to_owned(),
        Pattern::None => "none".to_owned(),
        Pattern::Wildcard => "_".to_owned(),
    };
    format!("{head}{bindings:?}")
}

fn sexp_stmt(s: &Stmt) -> String {
    match &s.kind {
        StmtKind::Let { name, ty, init } => {
            let ty = ty.as_ref().map_or("_".to_owned(), sexp_ty);
            let init = init.as_ref().map_or("_".to_owned(), sexp);
            format!("(let {} {ty} {init})", name.name)
        }
        StmtKind::LetTuple { names, value } => {
            let names: Vec<&str> = names.iter().map(|n| n.name.as_str()).collect();
            format!("(let-tuple [{}] {})", names.join(" "), sexp(value))
        }
        StmtKind::Assign { name, op, value } => {
            format!("({}= {} {})", assign_prefix(*op), name.name, sexp(value))
        }
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
        StmtKind::For {
            var,
            start,
            end,
            inclusive,
            step,
            body,
        } => format!(
            "({} {} {} {} {}{})",
            if *inclusive { "for=" } else { "for" },
            var.name,
            sexp(start),
            sexp(end),
            step.as_ref()
                .map_or(String::new(), |s| format!("step {} ", sexp(s))),
            sexp_block(body)
        ),
        StmtKind::PathAssign {
            name,
            path,
            op,
            value,
        } => {
            let steps: Vec<String> = path
                .iter()
                .map(|step| match step {
                    PathStep::Index(index) => sexp(index),
                    PathStep::Cell(row, col) => format!("{},{}", sexp(row), sexp(col)),
                    PathStep::Field(field) => format!(".{}", field.name),
                })
                .collect();
            format!(
                "(index{}= {} [{}] {})",
                assign_prefix(*op),
                name.name,
                steps.join(" "),
                sexp(value)
            )
        }
        StmtKind::ForEach { var, array, body } => {
            format!(
                "(foreach {} {} {})",
                var.name,
                sexp(array),
                sexp_block(body)
            )
        }
        StmtKind::Match { value, arms } => {
            let arms: Vec<String> = arms
                .iter()
                .map(|arm| format!("({:?} {})", arm_name(&arm.pattern), sexp_block(&arm.body)))
                .collect();
            format!("(match {} {})", sexp(value), arms.join(" "))
        }
        StmtKind::Break => "break".to_owned(),
        StmtKind::Continue => "continue".to_owned(),
        StmtKind::Return(Some(e)) => format!("(return {})", sexp(e)),
        StmtKind::Return(None) => "(return)".to_owned(),
        StmtKind::Expr(e) => sexp(e),
    }
}

/// The `+` of `+=`, or nothing for plain `=`.
fn assign_prefix(op: Option<BinOp>) -> &'static str {
    op.map_or("", bin_op)
}

fn bin_op(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

fn sexp_ty(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Known(ty) => type_name(*ty),
        TypeExpr::Param(name) => name.name.clone(),
        TypeExpr::Array(elem) => format!("[{}]", sexp_ty(elem)),
        TypeExpr::Optional(inner) => format!("{}?", sexp_ty(inner)),
        TypeExpr::Function { params, ret } => {
            let params: Vec<String> = params.iter().map(sexp_ty).collect();
            let ret = ret.as_ref().map_or("_".to_owned(), |t| sexp_ty(t));
            format!("fn({}) {ret}", params.join(", "))
        }
        TypeExpr::Applied { name, args } => {
            let args: Vec<String> = args.iter().map(sexp_ty).collect();
            format!("{}<{}>", name.name, args.join(", "))
        }
        TypeExpr::Tuple(items) => {
            let items: Vec<String> = items.iter().map(sexp_ty).collect();
            format!("({})", items.join(", "))
        }
    }
}

fn type_name(ty: Type) -> String {
    format!("{ty:?}").to_lowercase()
}

/// `<T: real|complex, U: int>`, or nothing for an ordinary function.
fn sexp_generics(f: &kmpl_ast::Function) -> String {
    if f.generics.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = f
        .generics
        .iter()
        .map(|g| {
            let bound: Vec<String> = g.bound.iter().map(|t| type_name(*t)).collect();
            format!("{}: {}", g.name.name, bound.join("|"))
        })
        .collect();
    format!("<{}>", parts.join(", "))
}

fn sexp(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(v) => v.to_string(),
        ExprKind::Real(v) => format!("{v:?}"),
        ExprKind::Complex { re, im } if *re == 0.0 => format!("{im:?}i"),
        ExprKind::Complex { re, im } => format!("({re:?}+{im:?}i)"),
        ExprKind::Bool(v) => v.to_string(),
        ExprKind::Str(text) => format!("{text:?}"),
        ExprKind::Var(id) => id.name.clone(),
        ExprKind::Unary { op, operand } => {
            let op = match op {
                UnOp::Neg => "-",
                UnOp::Not => "!",
                UnOp::BitNot => "~",
            };
            format!("({op} {})", sexp(operand))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            format!("({} {} {})", bin_op(*op), sexp(lhs), sexp(rhs))
        }
        ExprKind::Call { name, args } => {
            let args: Vec<String> = args.iter().map(sexp).collect();
            format!("(call {} [{}])", name.name, args.join(" "))
        }
        ExprKind::Tuple(items) => {
            let items: Vec<String> = items.iter().map(sexp).collect();
            format!("(tuple {})", items.join(" "))
        }
        ExprKind::Array(items) => {
            let items: Vec<String> = items.iter().map(sexp).collect();
            format!("(array {})", items.join(" "))
        }
        ExprKind::ArrayFill { value, len } => format!("(fill {} {})", sexp(value), sexp(len)),
        ExprKind::Index { base, index } => format!("(index {} {})", sexp(base), sexp(index)),
        ExprKind::Cell { base, row, col } => {
            format!("(cell {} {} {})", sexp(base), sexp(row), sexp(col))
        }
        ExprKind::Slice {
            base,
            start,
            end,
            inclusive,
        } => {
            let bound = |b: &Option<Box<Expr>>| b.as_ref().map_or("_".to_owned(), |e| sexp(e));
            format!(
                "({} {} {} {})",
                if *inclusive { "slice=" } else { "slice" },
                sexp(base),
                bound(start),
                bound(end)
            )
        }
        ExprKind::Swizzle { base, mask } => format!("(: {} {})", sexp(base), mask.name),
        ExprKind::Field { base, name } => format!("(. {} {})", sexp(base), name.name),
        ExprKind::StructLit { name, fields } => {
            let fields: Vec<String> = fields
                .iter()
                .map(|(field, value)| format!("({} {})", field.name, sexp(value)))
                .collect();
            format!("(new {} [{}])", name.name, fields.join(" "))
        }
        ExprKind::Variant {
            name,
            variant,
            args,
        } => {
            let args: Vec<String> = args.iter().map(sexp).collect();
            format!("({}::{} {})", name.name, variant.name, args.join(" "))
        }
        ExprKind::Some(value) => format!("(some {})", sexp(value)),
        ExprKind::None => "none".to_owned(),
        ExprKind::Lambda(l) => format!("(fn {} {})", l.params.len(), sexp_block(&l.body)),
    }
}
