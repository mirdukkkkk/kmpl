//! Tree-walking interpreter for checked kmpl programs.

mod eval;
mod ops;

#[cfg(test)]
mod tests;

pub use eval::MAX_CALL_DEPTH;

use kmpl_ast::{Program, Span};

pub use kmpl_builtins::Io;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeError {
    pub message: String,
    pub span: Span,
}

/// Stack for the interpreter thread. Each kmpl call uses several Rust
/// frames, so `MAX_CALL_DEPTH` calls need far more than the default 8 MiB.
/// Only touched pages are actually allocated.
const STACK_SIZE: usize = 1 << 30;

/// Runs `program`, which must have passed `kmpl_check::check`.
pub fn run(program: &Program, io: Io) -> Result<(), RuntimeError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("kmpl".to_owned())
            .stack_size(STACK_SIZE)
            .spawn_scoped(scope, move || eval::Interp::new(program, io).run(program))
            .expect("cannot start the interpreter thread")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}
