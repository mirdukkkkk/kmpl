//! Turns the IR into a native executable: LLVM produces an object file,
//! and the system linker joins it with the runtime (`kmpl_rt`).

mod arith;
mod boxed;
mod link;
mod llvm;
mod memory;
mod translate;
mod vector;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::process::{Command, ExitStatus};

use kmpl_ast::Span;

/// Compiles `module` into an executable at `output`. `locate` gives the text
/// shown under a runtime error at that span. The error is a message for the
/// user.
pub fn build(
    module: &kmpl_ir::Module,
    output: &Path,
    locate: &dyn Fn(Span) -> String,
) -> Result<(), String> {
    let object = llvm::object_code(module, locate)?;
    link::link(&object, output)
}

/// Compiles `module` to a temporary executable and runs it with `args`,
/// sharing this process's standard streams. Returns how the program ended.
pub fn run(
    module: &kmpl_ir::Module,
    locate: &dyn Fn(Span) -> String,
    args: &[String],
) -> Result<ExitStatus, String> {
    let dir = link::TempDir::new()?;
    let program = dir.path("program");
    build(module, &program, locate)?;
    Command::new(&program)
        .args(args)
        .status()
        .map_err(|e| format!("kmpl: cannot run the compiled program: {e}"))
}
