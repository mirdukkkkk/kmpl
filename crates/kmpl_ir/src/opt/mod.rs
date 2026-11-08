//! Optimizations on the IR. Run before `annotate`.
//!
//! None of them may change what a program does. In particular:
//! - an instruction that can stop the program with an error (`1 / 0`, `int`
//!   overflow, a bad index) stays, even if its result is unused;
//! - calls and built-ins keep their order;
//! - `real` arithmetic is never rearranged: floats are not associative.

use crate::Module;

pub fn optimize(_module: &mut Module) {}
