mod error;
mod infer;
mod init;
mod main_fn;
mod resolve;
mod returns;
mod types;

#[cfg(test)]
mod tests;

pub use error::CheckError;

use kmpl_ast::Program;

/// Runs all checks. Returns every error found, not just the first.
pub fn check(program: &Program) -> Result<(), Vec<CheckError>> {
    let mut errors = resolve::resolve(program);
    // Inference needs every name to exist, so it runs only after
    // resolution succeeded.
    let names_ok = errors.is_empty();
    errors.extend(init::check_init(program));
    if names_ok {
        let earlier_ok = errors.is_empty();
        errors.extend(infer::infer(program, earlier_ok));
    }
    errors.extend(returns::check_returns(program));
    errors.extend(main_fn::check_main(program));

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
