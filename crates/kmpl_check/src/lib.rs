mod error;
mod infer;
mod init;
mod literals;
mod main_fn;
mod resolve;
mod returns;
mod types;

#[cfg(test)]
mod tests;

pub use error::CheckError;

use kmpl_ast::Program;

/// Runs all checks. Returns every error found, not just the first.
///
/// On success, number literals in `program` are rewritten to their
/// inferred types (`1` used as `real` becomes `1.0`).
pub fn check(program: &mut Program) -> Result<(), Vec<CheckError>> {
    let mut errors = resolve::resolve(program);
    // Inference needs every name to exist, so it runs only after
    // resolution succeeded.
    let names_ok = errors.is_empty();
    errors.extend(init::check_init(program));

    let mut literal_types = None;
    if names_ok {
        let earlier_ok = errors.is_empty();
        let inference = infer::infer(program, earlier_ok);
        errors.extend(inference.errors);
        literal_types = Some(inference.literal_types);
    }

    errors.extend(returns::check_returns(program));
    errors.extend(main_fn::check_main(program));

    if !errors.is_empty() {
        return Err(errors);
    }
    if let Some(types) = literal_types {
        literals::rewrite(program, &types);
    }
    Ok(())
}
