//! The only `unsafe` in this crate: LLVM's "address of an element".
#![allow(unsafe_code)]

use inkwell::builder::{Builder, BuilderError};
use inkwell::types::{BasicType, IntType};
use inkwell::values::{IntValue, PointerValue};

/// The address of element `index` of a buffer of integers of type `int`.
pub fn element<'ctx>(
    builder: &Builder<'ctx>,
    int: IntType<'ctx>,
    buffer: PointerValue<'ctx>,
    index: u64,
) -> Result<PointerValue<'ctx>, BuilderError> {
    element_at(builder, int, buffer, int.const_int(index, false))
}

/// The address of element `index` of a buffer of values of type `ty`.
pub fn element_at<'ctx, T: BasicType<'ctx>>(
    builder: &Builder<'ctx>,
    ty: T,
    buffer: PointerValue<'ctx>,
    index: IntValue<'ctx>,
) -> Result<PointerValue<'ctx>, BuilderError> {
    // SAFETY: the address is only computed here. Callers keep `index` inside
    // the buffer: `buffer_slots` sizes argument buffers for the largest
    // operation, and arrays are bounds-checked first.
    unsafe { builder.build_in_bounds_gep(ty, buffer, &[index], "") }
}
