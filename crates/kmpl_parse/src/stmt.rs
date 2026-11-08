//! Statements and blocks.

use kmpl_ast::{
    BinOp, Block, Expr, ExprKind, Ident, PathStep, Span, Stmt, StmtKind, Type, TypeExpr,
};
use kmpl_lexer::Token;

use crate::error::PResult;
use crate::expr::{dotted_name, expr, header_expr};
use crate::token_stream::TokenStream;

/// `'{' stmt* '}'`. Returns the statements and the span from `{` to `}`.
pub fn block(ts: &mut TokenStream) -> PResult<(Block, Span)> {
    let open = ts.expect(Token::LBrace)?;

    let mut stmts = Vec::new();
    while ts.peek() != Some(&Token::RBrace) {
        if ts.at_end() {
            return Err(ts.error_expected("`}`"));
        }
        stmts.push(stmt(ts)?);
    }

    let close = ts.expect(Token::RBrace)?;
    Ok((stmts, open.to(close)))
}

pub fn stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    ts.enter()?;
    let parsed = stmt_kind(ts);
    ts.leave();
    parsed
}

fn stmt_kind(ts: &mut TokenStream) -> PResult<Stmt> {
    match ts.peek() {
        Some(Token::Let) => let_stmt(ts),
        Some(Token::If) => if_stmt(ts),
        Some(Token::While) => while_stmt(ts),
        Some(Token::For) => for_stmt(ts),
        Some(Token::Break) => keyword_stmt(ts, StmtKind::Break),
        Some(Token::Continue) => keyword_stmt(ts, StmtKind::Continue),
        Some(Token::Return) => return_stmt(ts),
        Some(Token::Ident(_)) if assign_op(ts.peek_nth(1)).is_some() => assign_stmt(ts),
        _ => expr_stmt(ts),
    }
}

/// `'let' IDENT (':' type)? ('=' expr)? ';'`, or a tuple to unpack:
/// `'let' '(' IDENT (',' IDENT)+ ','? ')' '=' expr ';'`.
fn let_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let start = ts.expect(Token::Let)?;
    if ts.peek() == Some(&Token::LParen) {
        let names = two_or_more(ts, |ts| ts.expect_ident())?;
        ts.expect(Token::Assign)?;
        let value = expr(ts)?;
        let end = ts.expect(Token::Semi)?;
        return Ok(Stmt {
            kind: StmtKind::LetTuple { names, value },
            span: start.to(end),
        });
    }

    let name = ts.expect_ident()?;

    let ty = match ts.eat(&Token::Colon) {
        Some(_) => Some(ty(ts)?),
        None => None,
    };

    let init = match ts.eat(&Token::Assign) {
        Some(_) => Some(expr(ts)?),
        None => None,
    };

    let end = ts.expect(Token::Semi)?;

    Ok(Stmt {
        kind: StmtKind::Let { name, ty, init },
        span: start.to(end),
    })
}

/// `'if' expr block ('else' (if | block))?`
///
/// `else if` becomes an `otherwise` block holding a single `If`.
fn if_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let start = ts.expect(Token::If)?;
    let cond = header_expr(ts)?;
    let (then, mut end) = block(ts)?;

    let otherwise = if ts.eat(&Token::Else).is_none() {
        None
    } else if ts.peek() == Some(&Token::If) {
        ts.enter()?;
        let nested = if_stmt(ts)?;
        ts.leave();
        end = nested.span;
        Some(vec![nested])
    } else {
        let (stmts, span) = block(ts)?;
        end = span;
        Some(stmts)
    };

    Ok(Stmt {
        kind: StmtKind::If {
            cond,
            then,
            otherwise,
        },
        span: start.to(end),
    })
}

/// `'while' expr block`
fn while_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let start = ts.expect(Token::While)?;
    let cond = header_expr(ts)?;
    let (body, end) = block(ts)?;

    Ok(Stmt {
        kind: StmtKind::While { cond, body },
        span: start.to(end),
    })
}

/// `'for' IDENT 'in' expr (('..' | '..=') expr ('step' expr)?)? block`
///
/// `step` is not a token, only a word with a meaning here, so a variable
/// can still be called `step`.
fn for_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let first = ts.expect(Token::For)?;
    let var = ts.expect_ident()?;
    ts.expect(Token::In)?;
    let start = header_expr(ts)?;

    let inclusive = match ts.peek() {
        Some(Token::DotDot) => false,
        Some(Token::DotDotEq) => true,
        _ => {
            let (body, last) = block(ts)?;
            return Ok(Stmt {
                kind: StmtKind::ForEach {
                    var,
                    array: start,
                    body,
                },
                span: first.to(last),
            });
        }
    };
    ts.next();

    let end = header_expr(ts)?;
    let step = match ts.peek() {
        Some(Token::Ident(word)) if word == "step" => {
            ts.next();
            Some(header_expr(ts)?)
        }
        _ => None,
    };
    let (body, last) = block(ts)?;

    Ok(Stmt {
        kind: StmtKind::For {
            var,
            start,
            end,
            inclusive,
            step,
            body,
        },
        span: first.to(last),
    })
}

/// `'break' ';'` and `'continue' ';'`
fn keyword_stmt(ts: &mut TokenStream, kind: StmtKind) -> PResult<Stmt> {
    let (_, start) = ts.next().expect("peeked");
    let end = ts.expect(Token::Semi)?;

    Ok(Stmt {
        kind,
        span: start.to(end),
    })
}

/// `'return' expr? ';'`
fn return_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let start = ts.expect(Token::Return)?;

    let value = match ts.peek() {
        Some(Token::Semi) => None,
        _ => Some(expr(ts)?),
    };

    let end = ts.expect(Token::Semi)?;

    Ok(Stmt {
        kind: StmtKind::Return(value),
        span: start.to(end),
    })
}

/// `=` is `Some(None)`, `+=` is `Some(Some(BinOp::Add))`, anything that
/// does not assign is `None`.
fn assign_op(token: Option<&Token>) -> Option<Option<BinOp>> {
    match token? {
        Token::Assign => Some(None),
        Token::PlusEq => Some(Some(BinOp::Add)),
        Token::MinusEq => Some(Some(BinOp::Sub)),
        Token::StarEq => Some(Some(BinOp::Mul)),
        Token::SlashEq => Some(Some(BinOp::Div)),
        _ => None,
    }
}

/// `IDENT assign_op expr ';'`
fn assign_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let name = ts.expect_ident()?;
    let op = assign_op(ts.peek()).expect("checked by stmt");
    ts.next();
    let value = expr(ts)?;
    let end = ts.expect(Token::Semi)?;

    let span = name.span.to(end);
    Ok(Stmt {
        kind: StmtKind::Assign { name, op, value },
        span,
    })
}

/// `expr ';'` or `IDENT ('[' expr ']' | '.' IDENT)+ assign_op expr ';'`. A path
/// can be any length, so the `=` is only visible after parsing it.
fn expr_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let e = expr(ts)?;
    if assign_op(ts.peek()).is_some() && is_path_target(&e) {
        return path_assign(ts, e);
    }

    let end = ts.expect(Token::Semi)?;

    let span = e.span.to(end);
    Ok(Stmt {
        kind: StmtKind::Expr(e),
        span,
    })
}

/// `a[i]`, `a.x`, `a[i].x`, but not `f()[i]`: there is no variable to change.
fn is_path_target(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Index { base, .. } | ExprKind::Field { base, .. } => {
            matches!(base.kind, ExprKind::Var(_)) || is_path_target(base)
        }
        _ => false,
    }
}

/// Parses the rest of `target = value;` or `target += value;`.
fn path_assign(ts: &mut TokenStream, target: Expr) -> PResult<Stmt> {
    let start = target.span;
    let mut path = Vec::new();
    let name = split_target(target, &mut path);

    let op = assign_op(ts.peek()).expect("checked by expr_stmt");
    ts.next();
    let value = expr(ts)?;
    let end = ts.expect(Token::Semi)?;

    Ok(Stmt {
        kind: StmtKind::PathAssign {
            name,
            path,
            op,
            value,
        },
        span: start.to(end),
    })
}

/// Splits `a[i].x` into `a` and its steps. `e` passed `is_path_target`.
fn split_target(e: Expr, path: &mut Vec<PathStep>) -> Ident {
    match e.kind {
        ExprKind::Index { base, index } => {
            let name = split_target(*base, path);
            path.push(PathStep::Index(*index));
            name
        }
        ExprKind::Field { base, name: field } => {
            let name = split_target(*base, path);
            path.push(PathStep::Field(field));
            name
        }
        ExprKind::Var(name) => name,
        _ => unreachable!("checked by is_path_target"),
    }
}

/// `'(' item (',' item)+ ','? ')'`
fn two_or_more<T>(
    ts: &mut TokenStream,
    item: fn(&mut TokenStream) -> PResult<T>,
) -> PResult<Vec<T>> {
    ts.expect(Token::LParen)?;
    let mut items = vec![item(ts)?];
    ts.expect(Token::Comma)?;
    loop {
        items.push(item(ts)?);
        if ts.eat(&Token::Comma).is_none() || ts.peek() == Some(&Token::RParen) {
            break;
        }
    }
    ts.expect(Token::RParen)?;
    Ok(items)
}

/// `type ::= name | '[' type ']' | '(' type (',' type)+ ','? ')'`. The
/// lexer gives type names as plain identifiers.
/// A built-in name becomes `Known`; any other name becomes `Param` (a
/// generic parameter, a struct or a typo), and the checker decides which.
pub fn ty(ts: &mut TokenStream) -> PResult<TypeExpr> {
    ts.enter()?;
    let parsed = ty_kind(ts);
    ts.leave();
    parsed
}

fn ty_kind(ts: &mut TokenStream) -> PResult<TypeExpr> {
    if ts.eat(&Token::LBracket).is_some() {
        let elem = ty(ts)?;
        ts.expect(Token::RBracket)?;
        return Ok(TypeExpr::Array(Box::new(elem)));
    }
    if ts.peek() == Some(&Token::LParen) {
        return Ok(TypeExpr::Tuple(two_or_more(ts, ty)?));
    }

    let name = dotted_name(ts)?;
    Ok(match Type::from_name(&name.name) {
        Some(ty) => TypeExpr::Known(ty),
        None => TypeExpr::Param(name),
    })
}
