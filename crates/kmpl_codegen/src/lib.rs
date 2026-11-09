//! Turns the IR into a native executable: LLVM produces an object file,
//! and the system linker joins it with the runtime (`kmpl_rt`).

mod link;
mod llvm;
mod supported;
mod translate;

use std::path::Path;

use kmpl_ast::Span;

pub use supported::UNSUPPORTED;

/// Compiles `module` into an executable at `output`. `locate` gives the text
/// shown under a runtime error at that span. The error is a message for the
/// user.
pub fn build(
    module: &kmpl_ir::Module,
    output: &Path,
    locate: &dyn Fn(Span) -> String,
) -> Result<(), String> {
    if !supported::is_supported(module) {
        return Err(UNSUPPORTED.to_owned());
    }
    let object = llvm::object_code(module, locate)?;
    link::link(&object, output)
}
