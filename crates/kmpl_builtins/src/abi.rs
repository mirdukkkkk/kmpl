//! What the generated code and `kmpl_rt` agree on.

/// One value crossing between generated code and the runtime.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Slot {
    pub tag: u64,
    pub bits: u64,
}

pub const TAG_INT: u64 = 0;
pub const TAG_REAL: u64 = 1;
pub const TAG_BOOL: u64 = 2;
