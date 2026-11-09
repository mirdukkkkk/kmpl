//! Top-level items: `const` and `fn`.

use kmpl_ast::{
    Const, EnumDef, EnumVariant, Function, Generic, Ident, Import, ImportNames, ImportedItem, Item,
    Param, Program, StructDef, StructField, Type,
};
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
    let mut public = Vec::new();
    while !ts.at_end() {
        let is_public = ts.eat(&Token::Pub).is_some();
        let parsed = item(ts)?;
        if is_public {
            public.push(item_name(&parsed).name.clone());
        }
        items.push(parsed);
    }
    Ok(Program {
        imports,
        items,
        public,
    })
}

fn item_name(item: &Item) -> &Ident {
    match item {
        Item::Const(c) => &c.name,
        Item::Function(f) => &f.name,
        Item::Struct(s) => &s.name,
        Item::Enum(e) => &e.name,
    }
}

/// `'import' (IDENT | '(' imported (',' imported)* ','? ')') 'from' STRING ';'`
fn import(ts: &mut TokenStream) -> PResult<Import> {
    let start = ts.expect(Token::Import)?;
    let names = match ts.eat(&Token::LParen) {
        Some(_) => ImportNames::Items(imported_items(ts)?),
        None => ImportNames::Module(ts.expect_ident()?),
    };

    if !at_word(ts, "from") {
        return Err(ts.error_expected("`from`"));
    }
    ts.next();

    let source_span = ts.peek_span();
    let Some(Token::Str(source)) = ts.peek().cloned() else {
        return Err(ts.error_expected("a string"));
    };
    ts.next();
    let end = ts.expect(Token::Semi)?;

    Ok(Import {
        names,
        source,
        source_span,
        span: start.to(end),
    })
}

/// `from` and `as` mean something only here, so they stay ordinary names.
fn at_word(ts: &TokenStream, word: &str) -> bool {
    matches!(ts.peek(), Some(Token::Ident(name)) if name == word)
}

/// The rest of `'(' imported (',' imported)* ','? ')'`.
fn imported_items(ts: &mut TokenStream) -> PResult<Vec<ImportedItem>> {
    let mut items = Vec::new();
    loop {
        let name = ts.expect_ident()?;
        let alias = match at_word(ts, "as") {
            true => {
                ts.next();
                ts.expect_ident()?
            }
            false => name.clone(),
        };
        items.push(ImportedItem { name, alias });

        if ts.eat(&Token::Comma).is_none() || ts.peek() == Some(&Token::RParen) {
            break;
        }
    }
    ts.expect(Token::RParen)?;
    Ok(items)
}

fn item(ts: &mut TokenStream) -> PResult<Item> {
    match ts.peek() {
        Some(Token::Const) => Ok(Item::Const(const_item(ts)?)),
        Some(Token::Fn) => Ok(Item::Function(function(ts)?)),
        Some(Token::Struct) => Ok(Item::Struct(struct_def(ts)?)),
        Some(Token::Enum) => Ok(Item::Enum(enum_def(ts)?)),
        _ => Err(ts.error_expected("`const`, `fn`, `struct` or `enum`")),
    }
}

/// `'enum' IDENT '{' variant (',' variant)* ','? '}'`, where
/// `variant ::= IDENT ('(' type (',' type)* ','? ')')?`
fn enum_def(ts: &mut TokenStream) -> PResult<EnumDef> {
    ts.expect(Token::Enum)?;
    let name = ts.expect_ident()?;
    let generics = type_params(ts)?;
    ts.expect(Token::LBrace)?;

    let mut variants = Vec::new();
    loop {
        let variant = ts.expect_ident()?;
        let mut fields = Vec::new();
        if ts.eat(&Token::LParen).is_some() {
            loop {
                fields.push(ty(ts)?);
                if ts.eat(&Token::Comma).is_none() || ts.peek() == Some(&Token::RParen) {
                    break;
                }
            }
            ts.expect(Token::RParen)?;
        }
        variants.push(EnumVariant {
            name: variant,
            fields,
        });
        if ts.eat(&Token::Comma).is_none() || ts.peek() == Some(&Token::RBrace) {
            break;
        }
    }
    ts.expect(Token::RBrace)?;

    Ok(EnumDef {
        name,
        generics,
        variants,
    })
}

/// `('<' IDENT (',' IDENT)* ','? '>')?`, on a struct or enum.
fn type_params(ts: &mut TokenStream) -> PResult<Vec<Ident>> {
    let mut params = Vec::new();
    if ts.eat(&Token::Lt).is_none() {
        return Ok(params);
    }
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
        params.push(name);
        if ts.eat(&Token::Comma).is_none() || ts.peek() == Some(&Token::Gt) {
            break;
        }
    }
    ts.expect(Token::Gt)?;
    Ok(params)
}

/// `'struct' IDENT '{' (IDENT ':' type (',' IDENT ':' type)* ','?)? '}'`
fn struct_def(ts: &mut TokenStream) -> PResult<StructDef> {
    ts.expect(Token::Struct)?;
    let name = ts.expect_ident()?;
    let generics = type_params(ts)?;
    ts.expect(Token::LBrace)?;

    let mut fields = Vec::new();
    while ts.peek() != Some(&Token::RBrace) {
        let field = ts.expect_ident()?;
        ts.expect(Token::Colon)?;
        fields.push(StructField {
            name: field,
            ty: ty(ts)?,
        });
        if ts.eat(&Token::Comma).is_none() {
            break;
        }
    }
    ts.expect(Token::RBrace)?;

    Ok(StructDef {
        name,
        generics,
        fields,
    })
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
