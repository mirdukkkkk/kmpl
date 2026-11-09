mod const_calls;
mod error;
mod generics;
mod infer;
mod init;
mod literals;
mod main_fn;
mod modules;
mod resolve;
mod returns;
mod types;

#[cfg(test)]
mod tests;

pub use error::CheckError;
pub use generics::MAX_INSTANCES;
pub use modules::{Module, ModuleImport, link};

use std::collections::HashMap;

use kmpl_ast::{Item, Program, Span, Ty};

/// The inferred type of every expression of a checked program, so later
/// stages do not need to work types out again.
#[derive(Debug, Default)]
pub struct TypeInfo {
    types: HashMap<(usize, Span), Ty>,
}

impl TypeInfo {
    /// The type of the expression at `span` in `program.items[item]`. The
    /// same span can appear in several copies of a generic function, so
    /// both are needed. Brackets share the span of what they contain.
    pub fn of(&self, item: usize, span: Span) -> Option<&Ty> {
        self.types.get(&(item, span))
    }
}

/// Runs all checks. Returns every error found, not just the first.
///
/// On success `program` is rewritten for execution:
/// - number literals have their inferred types (`1` used as `real` is `1.0`);
/// - every generic function is replaced by its copies (`lerp<real>`,
///   `lerp<vec2>`, ...), and calls name the copy they use.
pub fn check(program: &mut Program) -> Result<TypeInfo, Vec<CheckError>> {
    // Checks that do not depend on types run on the program as written.
    let mut errors = resolve::resolve(program);
    // Inference needs every name to exist, so it runs only after
    // resolution succeeded.
    let names_ok = errors.is_empty();
    errors.extend(init::check_init(program));
    errors.extend(returns::check_returns(program));
    errors.extend(main_fn::check_main(program));

    if !names_ok {
        return Err(errors);
    }
    errors.extend(const_calls::check_const_calls(program));

    match generics::instances(program) {
        Ok(copies) => program.items.extend(copies.into_iter().map(Item::Function)),
        Err(e) => {
            errors.extend(e);
            return Err(errors);
        }
    }

    let earlier_ok = errors.is_empty();
    // A call with a composite type needs a copy that a new round then checks,
    // and that copy may in turn make such calls.
    let inference = loop {
        let inference = infer::infer(program, earlier_ok);
        if !inference.errors.is_empty() {
            break inference;
        }
        match generics::add_composite(program, &inference.composite_calls) {
            Ok(true) => {}
            Ok(false) => break inference,
            Err(e) => {
                errors.extend(e);
                return Err(errors);
            }
        }
    };
    errors.extend(inference.errors.iter().cloned());
    if !errors.is_empty() {
        return Err(errors);
    }

    literals::rewrite(program, &inference);

    // Removing the generic originals moves later items, so the recorded
    // item indices have to follow.
    let mut new_index = Vec::with_capacity(program.items.len());
    let mut next = 0;
    for item in &program.items {
        new_index.push(next);
        if !is_generic(item) {
            next += 1;
        }
    }
    program.items.retain(|item| !is_generic(item));

    let types = inference
        .expr_types
        .into_iter()
        .map(|((item, span), ty)| ((new_index[item], span), ty))
        .collect();
    Ok(TypeInfo { types })
}

fn is_generic(item: &Item) -> bool {
    matches!(item, Item::Function(f) if !f.generics.is_empty())
}
