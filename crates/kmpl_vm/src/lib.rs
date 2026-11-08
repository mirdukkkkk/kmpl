//! Runs the IR.
//!
//! Registers live on one stack of values, and calls push explicit frames,
//! so deep recursion in a kmpl program does not use the Rust stack.
//!
//! The checker guarantees types, so a value of the wrong kind is a bug in
//! the checker or the lowering, and panics. Errors that depend on values
//! (division by zero, a bad index) are returned with their location.

mod ops;
mod vm;

#[cfg(test)]
mod tests;

pub use kmpl_builtins::Io;
pub use vm::{MAX_CALL_DEPTH, run};

use kmpl_ast::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeError {
    pub message: String,
    pub span: Span,
}
