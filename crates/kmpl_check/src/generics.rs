//! Monomorphization: every generic function gets one ordinary copy per
//! combination of types its bounds allow, with the generic names replaced.
//!
//! `fn lerp<T: real | vec2>(a: T, b: T, t: real) -> T` becomes
//! `lerp<real>` and `lerp<vec2>`. Inference then checks every copy like a
//! normal function, so the body is verified for every allowed type even
//! if a program only calls some of them. Calls are later renamed to the
//! copy they use, and the generic originals are dropped: the interpreter
//! (and later the compiler) only sees ordinary functions.

use std::collections::{HashMap, HashSet};

use kmpl_ast::{Block, Function, Ident, Item, Program, Span, StmtKind, Ty, Type, TypeExpr};

use crate::error::CheckError;
use crate::types::type_name;

/// Upper limit of copies per generic function, so a few unbounded
/// parameters (`<A, B, C, D>`: 8^4 = 4096 copies) do not blow up.
pub const MAX_INSTANCES: usize = 256;

/// The name of the copy for these types: `lerp<vec2>`, `f<int, real>`.
/// `<` cannot appear in an identifier, so it never clashes with user names.
pub fn instance_name(base: &str, types: &[TypeExpr]) -> String {
    format!("{base}<{}>", list_text(types))
}

fn list_text(types: &[TypeExpr]) -> String {
    let names: Vec<String> = types.iter().map(type_text).collect();
    names.join(", ")
}

fn type_text(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Known(ty) => type_name(*ty).to_owned(),
        TypeExpr::Param(name) => name.name.clone(),
        TypeExpr::Array(element) => format!("[{}]", type_text(element)),
        TypeExpr::Optional(inner) => format!("{}?", type_text(inner)),
        TypeExpr::Tuple(items) => format!("({})", list_text(items)),
        TypeExpr::Applied { name, args } => format!("{}<{}>", name.name, list_text(args)),
        TypeExpr::Function { params, ret } => match ret {
            Some(ret) => format!("fn({}) -> {}", list_text(params), type_text(ret)),
            None => format!("fn({})", list_text(params)),
        },
    }
}

/// A final type, as if it had been written in the source at `span`.
pub fn type_expr(ty: &Ty, span: Span) -> TypeExpr {
    let name_of = |name: &str| Ident {
        name: name.to_owned(),
        span,
    };
    let each = |items: &[Ty]| items.iter().map(|t| type_expr(t, span)).collect::<Vec<_>>();
    match ty {
        Ty::Prim(ty) => TypeExpr::Known(*ty),
        Ty::Array(element) => TypeExpr::Array(Box::new(type_expr(element, span))),
        Ty::Optional(inner) => TypeExpr::Optional(Box::new(type_expr(inner, span))),
        Ty::Tuple(items) => TypeExpr::Tuple(each(items)),
        Ty::Struct(name) => TypeExpr::Param(name_of(name)),
        Ty::Instance { name, args } => TypeExpr::Applied {
            name: name_of(name),
            args: each(args),
        },
        Ty::Function { params, ret } => TypeExpr::Function {
            params: each(params),
            ret: ret.as_ref().map(|t| Box::new(type_expr(t, span))),
        },
    }
}

/// Adds the copies that calls with arrays, structs and the like need. These
/// cannot be made in advance, as there is no end to such types. Returns
/// whether any were added: the new copies have to be checked in their turn.
pub fn add_composite(
    program: &mut Program,
    calls: &[(String, Vec<Ty>)],
) -> Result<bool, Vec<CheckError>> {
    let mut names: HashSet<String> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f.name.name.clone()),
            _ => None,
        })
        .collect();

    let mut copies = Vec::new();
    for (base, types) in calls {
        let original = program.items.iter().find_map(|item| match item {
            Item::Function(f) if f.name.name == *base => Some(f),
            _ => None,
        });
        let f = original.expect("a call names a function that exists");

        let written: Vec<TypeExpr> = types.iter().map(|t| type_expr(t, f.name.span)).collect();
        if !names.insert(instance_name(base, &written)) {
            continue;
        }

        let prefix = format!("{base}<");
        if names.iter().filter(|n| n.starts_with(&prefix)).count() > 2 * MAX_INSTANCES {
            return Err(vec![CheckError::new(
                format!("`{base}` needs too many copies, its calls keep making new types"),
                f.name.span,
            )]);
        }
        copies.push(instantiate(f, &written));
    }

    let added = !copies.is_empty();
    program.items.extend(copies.into_iter().map(Item::Function));
    Ok(added)
}

/// Builds the copies of every generic function in `program`.
pub fn instances(program: &Program) -> Result<Vec<Function>, Vec<CheckError>> {
    let mut copies = Vec::new();
    let mut errors = Vec::new();

    for item in &program.items {
        let Item::Function(f) = item else { continue };
        if f.generics.is_empty() {
            continue;
        }

        let count: usize = f.generics.iter().map(|g| g.bound.len()).product();
        if count > MAX_INSTANCES {
            errors.push(CheckError::new(
                format!(
                    "`{}` would need {count} copies, more than {MAX_INSTANCES}; narrow the bounds of its generic parameters",
                    f.name.name
                ),
                f.name.span,
            ));
            continue;
        }

        for types in combinations(
            &f.generics
                .iter()
                .map(|g| g.bound.as_slice())
                .collect::<Vec<_>>(),
        ) {
            let written: Vec<TypeExpr> = types.into_iter().map(TypeExpr::Known).collect();
            copies.push(instantiate(f, &written));
        }
    }

    if errors.is_empty() {
        Ok(copies)
    } else {
        Err(errors)
    }
}

/// Every way to pick one type from each bound, in order:
/// `[[int, real], [vec2]]` gives `[int, vec2]`, `[real, vec2]`.
fn combinations(bounds: &[&[Type]]) -> Vec<Vec<Type>> {
    let mut result = vec![Vec::new()];
    for bound in bounds {
        result = result
            .iter()
            .flat_map(|prefix| {
                bound.iter().map(move |ty| {
                    let mut next = prefix.clone();
                    next.push(*ty);
                    next
                })
            })
            .collect();
    }
    result
}

/// A copy of generic `f` with its parameters set to `types`.
fn instantiate(f: &Function, types: &[TypeExpr]) -> Function {
    let mapping: HashMap<&str, TypeExpr> = f
        .generics
        .iter()
        .zip(types)
        .map(|(g, ty)| (g.name.name.as_str(), ty.clone()))
        .collect();

    let mut copy = f.clone();
    copy.name.name = instance_name(&f.name.name, types);
    copy.generics.clear();
    for param in &mut copy.params {
        substitute(&mut param.ty, &mapping);
    }
    if let Some(ret) = &mut copy.ret {
        substitute(ret, &mapping);
    }
    substitute_block(&mut copy.body, &mapping);
    copy
}

fn substitute(ty: &mut TypeExpr, mapping: &HashMap<&str, TypeExpr>) {
    match ty {
        TypeExpr::Param(name) => {
            if let Some(known) = mapping.get(name.name.as_str()) {
                *ty = known.clone();
            }
        }
        TypeExpr::Applied { args, .. } => {
            for arg in args {
                substitute(arg, mapping);
            }
        }
        TypeExpr::Function { params, ret } => {
            for param in params {
                substitute(param, mapping);
            }
            if let Some(ret) = ret {
                substitute(ret, mapping);
            }
        }
        TypeExpr::Array(elem) | TypeExpr::Optional(elem) => substitute(elem, mapping),
        TypeExpr::Tuple(items) => {
            for item in items {
                substitute(item, mapping);
            }
        }
        TypeExpr::Known(_) => {}
    }
}

/// Generic names appear in a body only in `let x: T` annotations.
fn substitute_block(block: &mut Block, mapping: &HashMap<&str, TypeExpr>) {
    for stmt in block {
        match &mut stmt.kind {
            StmtKind::Let { ty: Some(ty), .. } => substitute(ty, mapping),
            StmtKind::If {
                then, otherwise, ..
            } => {
                substitute_block(then, mapping);
                if let Some(otherwise) = otherwise {
                    substitute_block(otherwise, mapping);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::ForEach { body, .. } => substitute_block(body, mapping),
            StmtKind::Match { arms, .. } => {
                for arm in arms {
                    substitute_block(&mut arm.body, mapping);
                }
            }
            _ => {}
        }
    }
}
