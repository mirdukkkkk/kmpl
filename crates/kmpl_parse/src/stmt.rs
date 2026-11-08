//! Statements and blocks.

use kmpl_ast::{Block, Expr, ExprKind, Ident, Span, Stmt, StmtKind, Type, TypeExpr};
use kmpl_lexer::Token;

use crate::error::PResult;
use crate::expr::expr;
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
    match ts.peek() {
        Some(Token::Let) => let_stmt(ts),
        Some(Token::If) => if_stmt(ts),
        Some(Token::While) => while_stmt(ts),
        Some(Token::For) => for_stmt(ts),
        Some(Token::Break) => keyword_stmt(ts, StmtKind::Break),
        Some(Token::Continue) => keyword_stmt(ts, StmtKind::Continue),
        Some(Token::Return) => return_stmt(ts),
        Some(Token::Ident(_)) if ts.peek_nth(1) == Some(&Token::Assign) => assign_stmt(ts),
        _ => expr_stmt(ts),
    }
}

/// `'let' IDENT (':' type)? ('=' expr)? ';'`
fn let_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let start = ts.expect(Token::Let)?;
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
    let cond = expr(ts)?;
    let (then, mut end) = block(ts)?;

    let otherwise = if ts.eat(&Token::Else).is_none() {
        None
    } else if ts.peek() == Some(&Token::If) {
        let nested = if_stmt(ts)?;
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
    let cond = expr(ts)?;
    let (body, end) = block(ts)?;

    Ok(Stmt {
        kind: StmtKind::While { cond, body },
        span: start.to(end),
    })
}

/// `'for' IDENT 'in' expr ('..' expr)? block`
fn for_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let first = ts.expect(Token::For)?;
    let var = ts.expect_ident()?;
    ts.expect(Token::In)?;
    let start = expr(ts)?;

    if ts.eat(&Token::DotDot).is_none() {
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

    let end = expr(ts)?;
    let (body, last) = block(ts)?;

    Ok(Stmt {
        kind: StmtKind::For {
            var,
            start,
            end,
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

/// `IDENT '=' expr ';'`
fn assign_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let name = ts.expect_ident()?;
    ts.expect(Token::Assign)?;
    let value = expr(ts)?;
    let end = ts.expect(Token::Semi)?;

    let span = name.span.to(end);
    Ok(Stmt {
        kind: StmtKind::Assign { name, value },
        span,
    })
}

/// `expr ';'` or `IDENT ('[' expr ']')+ '=' expr ';'`. An index expression
/// can be any length, so the `=` is only visible after parsing it.
fn expr_stmt(ts: &mut TokenStream) -> PResult<Stmt> {
    let e = expr(ts)?;
    if ts.peek() == Some(&Token::Assign) && is_index_target(&e) {
        return index_assign(ts, e);
    }

    let end = ts.expect(Token::Semi)?;

    let span = e.span.to(end);
    Ok(Stmt {
        kind: StmtKind::Expr(e),
        span,
    })
}

/// `a[i]`, `a[i][j]`, but not `f()[i]`: there is no variable to change.
fn is_index_target(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Index { base, .. } => {
            matches!(base.kind, ExprKind::Var(_)) || is_index_target(base)
        }
        _ => false,
    }
}

/// Parses the rest of `target = value;`.
fn index_assign(ts: &mut TokenStream, target: Expr) -> PResult<Stmt> {
    let start = target.span;
    let mut indices = Vec::new();
    let name = split_target(target, &mut indices);

    ts.expect(Token::Assign)?;
    let value = expr(ts)?;
    let end = ts.expect(Token::Semi)?;

    Ok(Stmt {
        kind: StmtKind::IndexAssign {
            name,
            indices,
            value,
        },
        span: start.to(end),
    })
}

/// Splits `a[i][j]` into `a` and `[i, j]`. `e` passed `is_index_target`.
fn split_target(e: Expr, indices: &mut Vec<Expr>) -> Ident {
    match e.kind {
        ExprKind::Index { base, index } => {
            let name = split_target(*base, indices);
            indices.push(*index);
            name
        }
        ExprKind::Var(name) => name,
        _ => unreachable!("checked by is_index_target"),
    }
}

/// `type ::= IDENT | '[' type ']'`. The lexer gives type names as plain
/// identifiers.
/// A built-in name becomes `Known`; any other name becomes `Param` (a
/// generic parameter or a typo), and the checker decides which.
pub fn ty(ts: &mut TokenStream) -> PResult<TypeExpr> {
    if ts.eat(&Token::LBracket).is_some() {
        let elem = ty(ts)?;
        ts.expect(Token::RBracket)?;
        return Ok(TypeExpr::Array(Box::new(elem)));
    }

    let name = ts.expect_ident()?;
    Ok(match Type::from_name(&name.name) {
        Some(ty) => TypeExpr::Known(ty),
        None => TypeExpr::Param(name),
    })
}
