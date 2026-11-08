mod error;
mod init;
mod main_fn;
mod resolve;

#[cfg(test)]
mod tests;

pub use error::CheckError;

use kmpl_ast::Program;

/// Runs all checks. Returns every error found, not just the first.
pub fn check(program: &Program) -> Result<(), Vec<CheckError>> {
    let mut errors = resolve::resolve(program);
    errors.extend(init::check_init(program));
    errors.extend(main_fn::check_main(program));

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
