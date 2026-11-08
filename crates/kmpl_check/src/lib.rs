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

use kmpl_ast::{Item, Program};

/// Runs all checks. Returns every error found, not just the first.
///
/// On success `program` is rewritten for execution:
/// - number literals have their inferred types (`1` used as `real` is `1.0`);
/// - every generic function is replaced by its copies (`lerp<real>`,
///   `lerp<vec2>`, ...), and calls name the copy they use.
pub fn check(program: &mut Program) -> Result<(), Vec<CheckError>> {
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

    match generics::instances(program) {
        Ok(copies) => program.items.extend(copies.into_iter().map(Item::Function)),
        Err(e) => {
            errors.extend(e);
            return Err(errors);
        }
    }

    let earlier_ok = errors.is_empty();
    let inference = infer::infer(program, earlier_ok);
    errors.extend(inference.errors.iter().cloned());
    if !errors.is_empty() {
        return Err(errors);
    }

    literals::rewrite(program, &inference);
    program
        .items
        .retain(|item| !matches!(item, Item::Function(f) if !f.generics.is_empty()));
    Ok(())
}
