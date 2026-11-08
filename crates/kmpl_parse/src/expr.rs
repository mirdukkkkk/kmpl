//! Expressions. One function per precedence level, weakest first,
//! exactly as in `grammar.ebnf`.

use kmpl_ast::{BinOp, Expr, ExprKind, Span, UnOp};
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
    let mut base = primary(ts)?;

    while ts.eat(&Token::LBracket).is_some() {
        base = index_or_slice(ts, base)?;
    }

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

fn is_range_op(token: Option<&Token>) -> bool {
    matches!(token, Some(Token::DotDot | Token::DotDotEq))
}

/// The rest of `base[`: `expr ']'`, or a slice with optional bounds.
/// `..=` needs an end: "up to and including" nothing means nothing.
fn index_or_slice(ts: &mut TokenStream, base: Expr) -> PResult<Expr> {
    let start = match is_range_op(ts.peek()) {
        true => None,
        false => Some(expr(ts)?),
    };

    if !is_range_op(ts.peek()) {
        let index = start.expect("no range operator, so there was an expression");
        let close = ts.expect(Token::RBracket)?;
        let span = base.span.to(close);
        return Ok(Expr {
            kind: ExprKind::Index {
                base: Box::new(base),
                index: Box::new(index),
            },
            span,
        });
    }

    let inclusive = ts.peek() == Some(&Token::DotDotEq);
    ts.next();
    let end = match !inclusive && ts.peek() == Some(&Token::RBracket) {
        true => None,
        false => Some(expr(ts)?),
    };
    let close = ts.expect(Token::RBracket)?;
    let span = base.span.to(close);

    Ok(Expr {
        kind: ExprKind::Slice {
            base: Box::new(base),
            start: start.map(Box::new),
            end: end.map(Box::new),
            inclusive,
        },
        span,
    })
}

fn primary(ts: &mut TokenStream) -> PResult<Expr> {
    let start = ts.peek_span();

    let kind = match ts.peek() {
        Some(Token::Int(v)) => ExprKind::Int(*v),
        Some(Token::Real(v)) => ExprKind::Real(*v),
        Some(Token::Imag(v)) => ExprKind::Complex { re: 0.0, im: *v },
        Some(Token::True) => ExprKind::Bool(true),
        Some(Token::False) => ExprKind::Bool(false),
        Some(Token::Ident(_)) => return var_or_call(ts),
        Some(Token::LBracket) => return array(ts),
        Some(Token::LParen) => {
            ts.next();
            let first = expr(ts)?;
            if ts.peek() == Some(&Token::Comma) {
                return tuple(ts, start, first);
            }
            ts.expect(Token::RParen)?;
            return Ok(first);
        }
        _ => return Err(ts.error_expected("expression")),
    };

    ts.next();
    Ok(Expr { kind, span: start })
}

/// The rest of `'(' expr (',' expr)+ ','? ')'`, after the first element.
fn tuple(ts: &mut TokenStream, open: Span, first: Expr) -> PResult<Expr> {
    let mut items = vec![first];
    while ts.eat(&Token::Comma).is_some() && ts.peek() != Some(&Token::RParen) {
        items.push(expr(ts)?);
    }
    if items.len() < 2 {
        return Err(ts.error_expected("expression"));
    }

    let close = ts.expect(Token::RParen)?;
    Ok(Expr {
        kind: ExprKind::Tuple(items),
        span: open.to(close),
    })
}

/// `'[' (expr (',' expr)* ','?)? ']'` or `'[' expr ';' expr ']'`
fn array(ts: &mut TokenStream) -> PResult<Expr> {
    let open = ts.expect(Token::LBracket)?;

    let mut items = Vec::new();
    while ts.peek() != Some(&Token::RBracket) {
        items.push(expr(ts)?);
        if items.len() == 1 && ts.eat(&Token::Semi).is_some() {
            let len = expr(ts)?;
            let close = ts.expect(Token::RBracket)?;
            let value = items.pop().expect("just pushed");
            return Ok(Expr {
                kind: ExprKind::ArrayFill {
                    value: Box::new(value),
                    len: Box::new(len),
                },
                span: open.to(close),
            });
        }
        if ts.eat(&Token::Comma).is_none() {
            break;
        }
    }

    let close = ts.expect(Token::RBracket)?;
    Ok(Expr {
        kind: ExprKind::Array(items),
        span: open.to(close),
    })
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
