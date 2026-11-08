mod error;
mod resolve;

#[cfg(test)]
mod tests;

pub use error::CheckError;

use kmpl_ast::Program;

/// Runs all checks. Returns every error found, not just the first.
pub fn check(program: &Program) -> Result<(), Vec<CheckError>> {
    let errors = resolve::resolve(program);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
