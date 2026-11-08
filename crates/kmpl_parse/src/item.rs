//! Top-level items: `const` and `fn`.

use kmpl_ast::{Const, Function, Item, Param, Program};
use kmpl_lexer::Token;

use crate::error::PResult;
use crate::expr::expr;
use crate::stmt::{block, ty};
use crate::token_stream::TokenStream;

/// `program ::= item*`
pub fn program(ts: &mut TokenStream) -> PResult<Program> {
    let mut items = Vec::new();
    while !ts.at_end() {
        items.push(item(ts)?);
    }
    Ok(Program { items })
}

fn item(ts: &mut TokenStream) -> PResult<Item> {
    match ts.peek() {
        Some(Token::Const) => Ok(Item::Const(const_item(ts)?)),
        Some(Token::Fn) => Ok(Item::Function(function(ts)?)),
        _ => Err(ts.error_expected("`const` or `fn`")),
    }
}

/// `'const' IDENT ':' type '=' expr ';'`
fn const_item(ts: &mut TokenStream) -> PResult<Const> {
    ts.expect(Token::Const)?;
    let name = ts.expect_ident()?;
    ts.expect(Token::Colon)?;
    let ty = ty(ts)?;
    ts.expect(Token::Assign)?;
    let value = expr(ts)?;
    ts.expect(Token::Semi)?;

    Ok(Const { name, ty, value })
}

/// `'fn' IDENT '(' params? ')' ('->' type)? block`
fn function(ts: &mut TokenStream) -> PResult<Function> {
    let start = ts.expect(Token::Fn)?;
    let name = ts.expect_ident()?;

    ts.expect(Token::LParen)?;
    let mut params = Vec::new();
    while ts.peek() != Some(&Token::RParen) {
        params.push(param(ts)?);
        if ts.eat(&Token::Comma).is_none() {
            break;
        }
    }
    ts.expect(Token::RParen)?;

    let ret = match ts.eat(&Token::Arrow) {
        Some(_) => Some(ty(ts)?),
        None => None,
    };

    let (body, end) = block(ts)?;

    Ok(Function {
        name,
        params,
        ret,
        body,
        span: start.to(end),
    })
}

/// `IDENT ':' type`
fn param(ts: &mut TokenStream) -> PResult<Param> {
    let name = ts.expect_ident()?;
    ts.expect(Token::Colon)?;
    let ty = ty(ts)?;
    Ok(Param { name, ty })
}
