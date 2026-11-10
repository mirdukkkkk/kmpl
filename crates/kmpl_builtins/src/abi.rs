//! What the generated code and `kmpl_rt` agree on.

use kmpl_ast::{BinOp, UnOp};

/// The numbers of a flat array, as generated code reads and writes them.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArrayView {
    pub data: *mut u8,
    pub len: u64,
}

/// The size of a `matrix`, whose numbers are an `ArrayView` of
/// `rows * cols`, row after row.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MatrixShape {
    pub rows: u64,
    pub cols: u64,
}

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
/// `bits` is a `*const Value` the caller keeps.
pub const TAG_BORROW: u64 = 3;
/// `bits` is a `*mut Value` from `Box`, and the receiver now owns it.
pub const TAG_MOVE: u64 = 4;
/// A step of a path: `bits` is the position of a field.
pub const TAG_FIELD: u64 = 5;

// The operations of `kmpl_rt_op`.
pub const OP_UNARY: u64 = 0;
pub const OP_BINARY: u64 = 1;
pub const OP_STRING: u64 = 2;
pub const OP_MAKE_ARRAY: u64 = 4;
pub const OP_ARRAY_FILL: u64 = 5;
pub const OP_ARRAY_LEN: u64 = 6;
pub const OP_INDEX: u64 = 7;
pub const OP_SLICE: u64 = 8;
pub const OP_MAKE_TUPLE: u64 = 9;
pub const OP_TUPLE_GET: u64 = 10;
pub const OP_MAKE_STRUCT: u64 = 11;
pub const OP_FIELD_GET: u64 = 12;
pub const OP_PATH_GET: u64 = 13;
pub const OP_PATH_SET: u64 = 14;
/// `extra` is the layout in the upper 32 bits and the variant in the lower.
pub const OP_MAKE_ENUM: u64 = 15;
pub const OP_MAKE_SOME: u64 = 16;
pub const OP_MAKE_NONE: u64 = 17;
pub const OP_VARIANT_OF: u64 = 18;
pub const OP_VARIANT_FIELD: u64 = 19;

// The forms of an array, for `OP_MAKE_ARRAY`.
pub const ARRAY_OF_VALUES: u64 = 0;
pub const ARRAY_OF_REALS: u64 = 1;
pub const ARRAY_OF_INTS: u64 = 2;

// The kinds of `kmpl_rt_box_vector`.
pub const KIND_COMPLEX: u64 = 0;
pub const KIND_VEC2: u64 = 1;
pub const KIND_VEC3: u64 = 2;
pub const KIND_VEC4: u64 = 3;

// The flags of `OP_SLICE`.
pub const SLICE_INCLUSIVE: u64 = 1;
pub const SLICE_HAS_START: u64 = 2;
pub const SLICE_HAS_END: u64 = 4;

const BIN_OPS: [BinOp; 18] = [
    BinOp::Add,
    BinOp::Sub,
    BinOp::Mul,
    BinOp::Div,
    BinOp::Eq,
    BinOp::Ne,
    BinOp::Lt,
    BinOp::Le,
    BinOp::Gt,
    BinOp::Ge,
    BinOp::And,
    BinOp::Or,
    BinOp::Rem,
    BinOp::BitAnd,
    BinOp::BitOr,
    BinOp::BitXor,
    BinOp::Shl,
    BinOp::Shr,
];

pub fn bin_op_code(op: BinOp) -> u64 {
    BIN_OPS
        .iter()
        .position(|candidate| *candidate == op)
        .expect("every operator is listed") as u64
}

pub fn bin_op(code: u64) -> BinOp {
    BIN_OPS[code as usize]
}

pub fn un_op_code(op: UnOp) -> u64 {
    match op {
        UnOp::Neg => 0,
        UnOp::Not => 1,
        UnOp::BitNot => 2,
    }
}

pub fn un_op(code: u64) -> UnOp {
    match code {
        0 => UnOp::Neg,
        1 => UnOp::Not,
        _ => UnOp::BitNot,
    }
}
