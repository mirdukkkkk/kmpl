//! Monomorphization: every generic function gets one ordinary copy per
//! combination of types its bounds allow, with the generic names replaced.
//!
//! `fn lerp<T: real | vec2>(a: T, b: T, t: real) -> T` becomes
//! `lerp<real>` and `lerp<vec2>`. Inference then checks every copy like a
//! normal function, so the body is verified for every allowed type even
//! if a program only calls some of them. Calls are later renamed to the
//! copy they use, and the generic originals are dropped: the interpreter
//! (and later the compiler) only sees ordinary functions.

use std::collections::HashMap;

use kmpl_ast::{Block, Function, Item, Program, StmtKind, Type, TypeExpr};

use crate::error::CheckError;
use crate::types::type_name;

/// Upper limit of copies per generic function, so a few unbounded
/// parameters (`<A, B, C, D>`: 7^4 = 2401 copies) do not blow up.
pub const MAX_INSTANCES: usize = 256;

/// The name of the copy for these types: `lerp<vec2>`, `f<int, real>`.
/// `<` cannot appear in an identifier, so it never clashes with user names.
pub fn instance_name(base: &str, types: &[Type]) -> String {
    let names: Vec<&str> = types.iter().map(|t| type_name(*t)).collect();
    format!("{base}<{}>", names.join(", "))
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
            copies.push(instantiate(f, &types));
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
fn instantiate(f: &Function, types: &[Type]) -> Function {
    let mapping: HashMap<&str, Type> = f
        .generics
        .iter()
        .zip(types)
        .map(|(g, ty)| (g.name.name.as_str(), *ty))
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

fn substitute(ty: &mut TypeExpr, mapping: &HashMap<&str, Type>) {
    match ty {
        TypeExpr::Param(name) => {
            if let Some(known) = mapping.get(name.name.as_str()) {
                *ty = TypeExpr::Known(*known);
            }
        }
        TypeExpr::Array(elem) => substitute(elem, mapping),
        TypeExpr::Tuple(items) => {
            for item in items {
                substitute(item, mapping);
            }
        }
        TypeExpr::Known(_) => {}
    }
}

/// Generic names appear in a body only in `let x: T` annotations.
fn substitute_block(block: &mut Block, mapping: &HashMap<&str, Type>) {
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
            _ => {}
        }
    }
}
