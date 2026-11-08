//! Top-level items: `const` and `fn`.

use kmpl_ast::{Const, Function, Generic, Import, Item, Param, Program, Type};
use kmpl_lexer::Token;

use crate::error::{PResult, ParseError};
use crate::expr::expr;
use crate::stmt::{block, ty};
use crate::token_stream::TokenStream;

/// `program ::= import* item*`
pub fn program(ts: &mut TokenStream) -> PResult<Program> {
    let mut imports = Vec::new();
    while ts.peek() == Some(&Token::Import) {
        imports.push(import(ts)?);
    }

    let mut items = Vec::new();
    while !ts.at_end() {
        items.push(item(ts)?);
    }
    Ok(Program { imports, items })
}

/// `'import' IDENT ('.' IDENT)* ';'`
fn import(ts: &mut TokenStream) -> PResult<Import> {
    let start = ts.expect(Token::Import)?;
    let mut path = vec![ts.expect_ident()?];
    while ts.eat(&Token::Dot).is_some() {
        path.push(ts.expect_ident()?);
    }
    let end = ts.expect(Token::Semi)?;

    Ok(Import {
        path,
        span: start.to(end),
    })
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

/// `'fn' IDENT generics? '(' params? ')' ('->' type)? block`
fn function(ts: &mut TokenStream) -> PResult<Function> {
    let start = ts.expect(Token::Fn)?;
    let name = ts.expect_ident()?;
    let generics = if ts.peek() == Some(&Token::Lt) {
        generics(ts)?
    } else {
        Vec::new()
    };

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
        generics,
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

/// `'<' generic (',' generic)* ','? '>'` with `generic ::= IDENT (':' bound)?`
fn generics(ts: &mut TokenStream) -> PResult<Vec<Generic>> {
    ts.expect(Token::Lt)?;
    let mut generics = Vec::new();
    loop {
        let name = ts.expect_ident()?;
        if Type::from_name(&name.name).is_some() {
            return Err(ParseError::new(
                format!(
                    "`{}` is a built-in type, choose another name for the generic parameter",
                    name.name
                ),
                name.span,
            ));
        }
        let bound = match ts.eat(&Token::Colon) {
            Some(_) => bound(ts)?,
            None => Type::ALL.to_vec(),
        };
        generics.push(Generic { name, bound });

        if ts.eat(&Token::Comma).is_none() || ts.peek() == Some(&Token::Gt) {
            break;
        }
    }
    ts.expect(Token::Gt)?;
    Ok(generics)
}

/// `bound ::= bound_item ('|' bound_item)*` where an item is a type name or
/// a group: `number` (`int | real | complex`), `float` (`real | complex`),
/// `vector` (`vec2 | vec3 | vec4`).
fn bound(ts: &mut TokenStream) -> PResult<Vec<Type>> {
    let mut types = Vec::new();
    loop {
        let name = ts.expect_ident()?;
        let group: &[Type] = match name.name.as_str() {
            "number" => &[Type::Int, Type::Real, Type::Complex],
            "float" => &[Type::Real, Type::Complex],
            "vector" => &[Type::Vec2, Type::Vec3, Type::Vec4],
            other => match Type::from_name(other) {
                Some(ty) => &[ty][..],
                None => {
                    return Err(ParseError::new(
                        format!(
                            "unknown type `{other}` in bound, use a type or `number`, `float`, `vector`"
                        ),
                        name.span,
                    ));
                }
            },
        };
        for ty in group {
            if !types.contains(ty) {
                types.push(*ty);
            }
        }
        if ts.eat(&Token::Pipe).is_none() {
            return Ok(types);
        }
    }
}
