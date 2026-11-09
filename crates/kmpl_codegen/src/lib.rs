//! Turns the IR into a native executable: LLVM produces an object file,
//! and the system linker joins it with the runtime (`kmpl_rt`).

mod link;
mod llvm;

use std::path::Path;

/// Compiles `module` into an executable at `output`. The error is a message
/// for the user.
pub fn build(module: &kmpl_ir::Module, output: &Path) -> Result<(), String> {
    let object = llvm::object_code(module)?;
    link::link(&object, output)
}
