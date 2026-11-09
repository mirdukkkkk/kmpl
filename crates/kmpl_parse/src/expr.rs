//! Expressions. One function per precedence level, weakest first,
//! exactly as in `grammar.ebnf`.

use kmpl_ast::{BinOp, Expr, ExprKind, Ident, Span, UnOp};
use kmpl_lexer::Token;

use crate::error::{PResult, ParseError};
use crate::token_stream::TokenStream;

pub fn expr(ts: &mut TokenStream) -> PResult<Expr> {
    ts.enter()?;
    let e = or(ts);
    ts.leave();
    e
}

/// An expression in the header of `if`, `while` or `for`.
pub fn header_expr(ts: &mut TokenStream) -> PResult<Expr> {
    ts.struct_literals = false;
    let e = expr(ts);
    ts.struct_literals = true;
    e
}

/// An expression inside brackets, where a struct literal is never in doubt.
fn inner_expr(ts: &mut TokenStream) -> PResult<Expr> {
    let outer = std::mem::replace(&mut ts.struct_literals, true);
    let e = expr(ts);
    ts.struct_literals = outer;
    e
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
    left_assoc(ts, bit_or, |tok| match tok {
        Token::Lt => Some(BinOp::Lt),
        Token::Le => Some(BinOp::Le),
        Token::Gt => Some(BinOp::Gt),
        Token::Ge => Some(BinOp::Ge),
        _ => None,
    })
}

fn bit_or(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, bit_xor, |tok| match tok {
        Token::Pipe => Some(BinOp::BitOr),
        _ => None,
    })
}

fn bit_xor(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, bit_and, |tok| match tok {
        Token::Caret => Some(BinOp::BitXor),
        _ => None,
    })
}

fn bit_and(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, shift, |tok| match tok {
        Token::Amp => Some(BinOp::BitAnd),
        _ => None,
    })
}

fn shift(ts: &mut TokenStream) -> PResult<Expr> {
    left_assoc(ts, add, |tok| match tok {
        Token::Shl => Some(BinOp::Shl),
        Token::Shr => Some(BinOp::Shr),
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
        Token::Percent => Some(BinOp::Rem),
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
    // The smallest `int` is the one number whose digits alone do not fit.
    if ts.peek() == Some(&Token::Minus) && ts.peek_nth(1) == Some(&Token::MinIntDigits) {
        let (_, minus) = ts.next().expect("peeked");
        let (_, digits) = ts.next().expect("peeked");
        return Ok(Expr {
            kind: ExprKind::Int(i64::MIN),
            span: minus.to(digits),
        });
    }

    let op = match ts.peek() {
        Some(Token::Bang) => UnOp::Not,
        Some(Token::Minus) => UnOp::Neg,
        Some(Token::Tilde) => UnOp::BitNot,
        _ => return postfix(ts),
    };

    let (_, op_span) = ts.next().expect("peeked");
    ts.enter()?;
    let operand = unary(ts)?;
    ts.leave();
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

    loop {
        if ts.eat(&Token::LBracket).is_some() {
            base = index_or_slice(ts, base)?;
        } else if ts.eat(&Token::Dot).is_some() {
            let name = ts.expect_ident()?;
            if ts.eat(&Token::LParen).is_some() {
                base = method_call(ts, base, name)?;
                continue;
            }
            let span = base.span.to(name.span);
            base = Expr {
                kind: ExprKind::Field {
                    base: Box::new(base),
                    name,
                },
                span,
            };
        } else {
            break;
        }
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

/// The rest of `base '.' name '(' args ')'`, which is `name(base, args)`.
fn method_call(ts: &mut TokenStream, base: Expr, name: Ident) -> PResult<Expr> {
    let (mut args, close) = call_args(ts)?;
    let span = base.span.to(close);
    args.insert(0, base);
    Ok(Expr {
        kind: ExprKind::Call { name, args },
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
        false => Some(inner_expr(ts)?),
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
        false => Some(inner_expr(ts)?),
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
        Some(Token::Str(text)) => ExprKind::Str(text.clone()),
        Some(Token::MinIntDigits) => {
            return Err(ParseError::new(
                "integer literal `9223372036854775808` is too large for `int`",
                start,
            ));
        }
        Some(Token::KwNone) => ExprKind::None,
        Some(Token::KwSome) => return some(ts),
        Some(Token::Ident(_)) => return var_or_call(ts),
        Some(Token::LBracket) => return array(ts),
        Some(Token::LParen) => {
            ts.next();
            let first = inner_expr(ts)?;
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

/// `'some' '(' expr ')'`
fn some(ts: &mut TokenStream) -> PResult<Expr> {
    let start = ts.expect(Token::KwSome)?;
    ts.expect(Token::LParen)?;
    let value = inner_expr(ts)?;
    let close = ts.expect(Token::RParen)?;
    Ok(Expr {
        kind: ExprKind::Some(Box::new(value)),
        span: start.to(close),
    })
}

/// The rest of `'(' expr (',' expr)+ ','? ')'`, after the first element.
fn tuple(ts: &mut TokenStream, open: Span, first: Expr) -> PResult<Expr> {
    let mut items = vec![first];
    while ts.eat(&Token::Comma).is_some() && ts.peek() != Some(&Token::RParen) {
        items.push(inner_expr(ts)?);
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
        items.push(inner_expr(ts)?);
        if items.len() == 1 && ts.eat(&Token::Semi).is_some() {
            let len = inner_expr(ts)?;
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

/// `IDENT ('.' IDENT)?`. `stats.mean` becomes one `Ident` with that name;
/// the module linker works out what it refers to.
pub fn dotted_name(ts: &mut TokenStream) -> PResult<Ident> {
    let first = ts.expect_ident()?;
    if ts.eat(&Token::Dot).is_none() {
        return Ok(first);
    }

    let member = ts.expect_ident()?;
    Ok(Ident {
        name: format!("{}.{}", first.name, member.name),
        span: first.span.to(member.span),
    })
}

/// Is the next thing `a.b(`, `a.B {` or `a.B::`? Those name an item of a module.
/// Any other `a.b` reads a field, which `postfix` handles.
fn at_module_item(ts: &TokenStream) -> bool {
    ts.peek_nth(1) == Some(&Token::Dot)
        && matches!(ts.peek_nth(2), Some(Token::Ident(_)))
        && match ts.peek_nth(3) {
            Some(Token::LParen | Token::ColonColon) => true,
            Some(Token::LBrace) => ts.struct_literals,
            _ => false,
        }
}

fn var_or_call(ts: &mut TokenStream) -> PResult<Expr> {
    let name = match at_module_item(ts) {
        true => dotted_name(ts)?,
        false => ts.expect_ident()?,
    };

    if ts.eat(&Token::ColonColon).is_some() {
        return variant(ts, name);
    }

    if ts.struct_literals && ts.peek() == Some(&Token::LBrace) {
        return struct_literal(ts, name);
    }

    if ts.eat(&Token::LParen).is_none() {
        let span = name.span;
        return Ok(Expr {
            kind: ExprKind::Var(name),
            span,
        });
    }

    let (args, close) = call_args(ts)?;
    let span = name.span.to(close);

    Ok(Expr {
        kind: ExprKind::Call { name, args },
        span,
    })
}

/// The rest of a call after `(`: the arguments and the span of `)`.
fn call_args(ts: &mut TokenStream) -> PResult<(Vec<Expr>, Span)> {
    let mut args = Vec::new();
    while ts.peek() != Some(&Token::RParen) {
        args.push(inner_expr(ts)?);
        if ts.eat(&Token::Comma).is_none() {
            break;
        }
    }
    let close = ts.expect(Token::RParen)?;
    Ok((args, close))
}

/// The rest of `name '::' IDENT ('(' args ')')?`.
fn variant(ts: &mut TokenStream, name: Ident) -> PResult<Expr> {
    let variant = ts.expect_ident()?;
    let (args, end) = match ts.eat(&Token::LParen) {
        Some(_) => call_args(ts)?,
        None => (Vec::new(), variant.span),
    };
    let span = name.span.to(end);
    Ok(Expr {
        kind: ExprKind::Variant {
            name,
            variant,
            args,
        },
        span,
    })
}

/// The rest of `name '{' (IDENT ':' expr (',' IDENT ':' expr)* ','?)? '}'`.
fn struct_literal(ts: &mut TokenStream, name: Ident) -> PResult<Expr> {
    ts.expect(Token::LBrace)?;

    let mut fields = Vec::new();
    while ts.peek() != Some(&Token::RBrace) {
        let field = ts.expect_ident()?;
        ts.expect(Token::Colon)?;
        fields.push((field, inner_expr(ts)?));
        if ts.eat(&Token::Comma).is_none() {
            break;
        }
    }

    let close = ts.expect(Token::RBrace)?;
    let span = name.span.to(close);
    Ok(Expr {
        kind: ExprKind::StructLit { name, fields },
        span,
    })
}
