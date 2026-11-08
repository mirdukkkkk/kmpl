//! Expressions. One function per precedence level, weakest first,
//! exactly as in `grammar.ebnf`.

use kmpl_ast::{BinOp, Expr, ExprKind, UnOp};
use kmpl_lexer::Token;

use crate::error::PResult;
use crate::token_stream::TokenStream;

pub fn expr(ts: &mut TokenStream) -> PResult<Expr> {
    or(ts)
}

fn or(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, and, |tok| match tok {
        Token::Or => Some(BinOp::Or),
        _ => None,
    })
}

fn and(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, eq, |tok| match tok {
        Token::And => Some(BinOp::And),
        _ => None,
    })
}

fn eq(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, comp, |tok| match tok {
        Token::EqEq => Some(BinOp::Eq),
        Token::NotEq => Some(BinOp::Ne),
        _ => None,
    })
}

fn comp(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, add, |tok| match tok {
        Token::Lt => Some(BinOp::Lt),
        Token::Le => Some(BinOp::Le),
        Token::Gt => Some(BinOp::Gt),
        Token::Ge => Some(BinOp::Ge),
        _ => None,
    })
}

fn add(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, mul, |tok| match tok {
        Token::Plus => Some(BinOp::Add),
        Token::Minus => Some(BinOp::Sub),
        _ => None,
    })
}

fn mul(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, unary, |tok| match tok {
        Token::Star => Some(BinOp::Mul),
        Token::Slash => Some(BinOp::Div),
        _ => None,
    })
}

/// `operand (op operand)*`, folded to the left: `a - b - c` is `(a - b) - c`.
fn left_assoc(
    ts: &mut TokenStream,
    operand: fn(&mut TokenStream) -> PResult<Expr>,
    op_of: fn(&Token) -> Option<BinOp>,
) -> PResult<Expr> {
    let mut lhs = operand(ts)?;

    while let Some(op) = ts.peek().and_then(op_of) {
        ts.next();
        let rhs = operand(ts)?;
        let span = lhs.span.to(rhs.span);
        lhs = Expr {
            kind: ExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        };
    }

    Ok(lhs)
}

fn unary(ts: &mut TokenStream) -> PResult<Expr> {
    let op = match ts.peek() {
        Some(Token::Bang) => UnOp::Not,
        Some(Token::Minus) => UnOp::Neg,
        _ => return postfix(ts),
    };

    let (_, op_span) = ts.next().expect("peeked");
    let operand = unary(ts)?;
    let span = op_span.to(operand.span);

    Ok(Expr {
        kind: ExprKind::Unary {
            op,
            operand: Box::new(operand),
        },
        span,
    })
}

fn postfix(ts: &mut TokenStream) -> PResult<Expr> {
    let base = primary(ts)?;

    if ts.eat(&Token::Colon).is_none() {
        return Ok(base);
    }

    let mask = ts.expect_ident()?;
    let span = base.span.to(mask.span);

    Ok(Expr {
        kind: ExprKind::Swizzle {
            base: Box::new(base),
            mask,
        },
        span,
    })
}

fn primary(ts: &mut TokenStream) -> PResult<Expr> {
    let start = ts.peek_span();

    let kind = match ts.peek() {
        Some(Token::Int(v)) => ExprKind::Int(*v),
        Some(Token::Real(v)) => ExprKind::Real(*v),
        Some(Token::Imag(v)) => ExprKind::Imag(*v),
        Some(Token::True) => ExprKind::Bool(true),
        Some(Token::False) => ExprKind::Bool(false),
        Some(Token::Ident(_)) => return var_or_call(ts),
        Some(Token::LParen) => {
            ts.next();
            let inner = expr(ts)?;
            ts.expect(Token::RParen)?;
            return Ok(inner);
        }
        _ => return Err(ts.error_expected("expression")),
    };

    ts.next();
    Ok(Expr { kind, span: start })
}

fn var_or_call(ts: &mut TokenStream) -> PResult<Expr> {
    let name = ts.expect_ident()?;

    if ts.eat(&Token::LParen).is_none() {
        let span = name.span;
        return Ok(Expr {
            kind: ExprKind::Var(name),
            span,
        });
    }

    let mut args = Vec::new();
    while ts.peek() != Some(&Token::RParen) {
        args.push(expr(ts)?);
        if ts.eat(&Token::Comma).is_none() {
            break;
        }
    }
    let close = ts.expect(Token::RParen)?;
    let span = name.span.to(close);

    Ok(Expr {
        kind: ExprKind::Call { name, args },
        span,
    })
}
