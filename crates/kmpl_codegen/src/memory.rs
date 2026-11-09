//! The only `unsafe` in this crate: LLVM's "address of an element".
#![allow(unsafe_code)]

use inkwell::builder::{Builder, BuilderError};
use inkwell::types::IntType;
use inkwell::values::PointerValue;

/// The address of element `index` of a buffer of integers of type `int`.
pub fn element<'ctx>(
    builder: &Builder<'ctx>,
    int: IntType<'ctx>,
    buffer: PointerValue<'ctx>,
    index: u64,
) -> Result<PointerValue<'ctx>, BuilderError> {
    let index = int.const_int(index, false);
    // SAFETY: callers keep `index` inside the buffer they allocated, which
    // `buffer_slots` sizes for the largest operation of the function.
    unsafe { builder.build_in_bounds_gep(int, buffer, &[index], "") }
}
