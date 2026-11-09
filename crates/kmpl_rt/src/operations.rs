//! What generated code asks the runtime to do with boxed values.

use std::sync::Arc;

use kmpl_builtins::abi::{self, Slot};
use kmpl_builtins::{StructValue, Value, ops};

use crate::{STATE, fail, text};

/// An owned or shared value out of a slot.
///
/// # Safety
/// A `TAG_BORROW` or `TAG_MOVE` slot must hold a valid pointer, and a
/// `TAG_MOVE` one must not be used again.
pub unsafe fn value(slot: Slot) -> Value {
    match slot.tag {
        abi::TAG_INT => Value::Int(slot.bits as i64),
        abi::TAG_REAL => Value::Real(f64::from_bits(slot.bits)),
        abi::TAG_BOOL => Value::Bool(slot.bits != 0),
        // SAFETY: promised by the caller.
        abi::TAG_BORROW => unsafe { (*(slot.bits as *const Value)).clone() },
        // SAFETY: promised by the caller.
        abi::TAG_MOVE => unsafe { *Box::from_raw(slot.bits as *mut Value) },
        tag => panic!("bad slot tag {tag}"),
    }
}

pub fn to_slot(value: Value) -> Slot {
    match value {
        Value::Int(v) => Slot {
            tag: abi::TAG_INT,
            bits: v as u64,
        },
        Value::Real(v) => Slot {
            tag: abi::TAG_REAL,
            bits: v.to_bits(),
        },
        Value::Bool(v) => Slot {
            tag: abi::TAG_BOOL,
            bits: u64::from(v),
        },
        other => Slot {
            tag: abi::TAG_MOVE,
            bits: Box::into_raw(Box::new(other)) as u64,
        },
    }
}

/// # Safety
/// `slot` is a `TAG_BORROW` slot with a pointer valid for `'a`.
unsafe fn borrowed<'a>(slot: Slot) -> &'a Value {
    assert_eq!(slot.tag, abi::TAG_BORROW);
    // SAFETY: promised by the caller.
    unsafe { &*(slot.bits as *const Value) }
}

fn int(slot: Slot) -> i64 {
    slot.bits as i64
}

/// # Safety
/// The slot's two words describe a constant text.
unsafe fn slot_text(slot: Slot) -> String {
    // SAFETY: promised by the caller.
    unsafe { text(slot.tag as *const u8, slot.bits) }
}

/// # Safety
/// `args` holds what the code generator passes for operation `code`.
pub unsafe fn operation(code: u64, extra: u64, args: &[Slot]) -> Result<Value, String> {
    // SAFETY: promised by the caller.
    unsafe {
        let values = |slots: &[Slot]| -> Vec<Value> { slots.iter().map(|s| value(*s)).collect() };
        Ok(match code {
            abi::OP_UNARY => ops::unary(abi::un_op(extra), value(args[0]))?,
            abi::OP_BINARY => ops::binary(abi::bin_op(extra), value(args[0]), value(args[1]))?,
            abi::OP_STRING => Value::Str(slot_text(args[0]).into()),
            abi::OP_MAKE_ARRAY => Value::Array(Arc::new(values(args))),
            abi::OP_ARRAY_FILL => ops::array_fill(value(args[0]), int(args[1]))?,
            abi::OP_ARRAY_LEN => Value::Int(borrowed(args[0]).as_array().len() as i64),
            abi::OP_INDEX => {
                let items = borrowed(args[0]).as_array();
                items[ops::checked_index(int(args[1]), items.len())?].clone()
            }
            abi::OP_SLICE => slice(extra, args)?,
            abi::OP_MAKE_TUPLE => Value::Tuple(values(args).into()),
            abi::OP_TUPLE_GET => match borrowed(args[0]) {
                Value::Tuple(items) => items[extra as usize].clone(),
                other => panic!("unpacking {other:?}"),
            },
            abi::OP_MAKE_STRUCT => Value::Struct(Arc::new(StructValue {
                layout: STATE.with_borrow(|state| state.layouts[extra as usize].clone()),
                values: values(args),
            })),
            abi::OP_FIELD_GET => match borrowed(args[0]) {
                Value::Struct(s) => s.values[extra as usize].clone(),
                other => panic!("a field of {other:?}"),
            },
            abi::OP_PATH_GET => read_path(borrowed(args[0]), &args[1..]),
            abi::OP_PATH_SET => {
                let mut root = value(args[0]);
                write_path(&mut root, &args[2..], value(args[1]));
                root
            }
            other => panic!("unknown operation {other}"),
        })
    }
}

/// # Safety
/// See `operation`.
unsafe fn slice(flags: u64, args: &[Slot]) -> Result<Value, String> {
    // SAFETY: promised by the caller.
    let items = unsafe { borrowed(args[0]) }.as_array();
    let mut bounds = args[1..].iter();
    let mut next_if = |flag| (flags & flag != 0).then(|| int(*bounds.next().expect("passed")));

    let from = next_if(abi::SLICE_HAS_START).unwrap_or(0);
    let (to, inclusive) = match next_if(abi::SLICE_HAS_END) {
        Some(end) => (end, flags & abi::SLICE_INCLUSIVE != 0),
        None => (items.len() as i64, false),
    };
    let range = ops::slice_bounds(from, to, inclusive, items.len())?;
    Ok(Value::Array(Arc::new(items[range].to_vec())))
}

/// `i` as a position, or stops the program at the location in `location`.
///
/// # Safety
/// See `slot_text`.
unsafe fn index(slot: Slot, location: Slot, len: usize) -> usize {
    ops::checked_index(int(slot), len).unwrap_or_else(|message| {
        // SAFETY: promised by the caller.
        fail(&message, &unsafe { slot_text(location) })
    })
}

/// # Safety
/// See `operation`.
unsafe fn read_path(root: &Value, mut steps: &[Slot]) -> Value {
    let mut value = root.clone();
    while let [step, rest @ ..] = steps {
        let element = match (step.tag, &value) {
            (abi::TAG_FIELD, Value::Struct(s)) => {
                steps = rest;
                s.values[step.bits as usize].clone()
            }
            (_, Value::Array(items)) => {
                // SAFETY: promised by the caller.
                let i = unsafe { index(*step, rest[0], items.len()) };
                steps = &rest[1..];
                items[i].clone()
            }
            (_, other) => panic!("bad target {other:?}"),
        };
        value = element;
    }
    value
}

/// # Safety
/// See `operation`.
unsafe fn write_path(root: &mut Value, mut steps: &[Slot], value: Value) {
    let mut target = root;
    while let [step, rest @ ..] = steps {
        target = match (step.tag, target) {
            (abi::TAG_FIELD, Value::Struct(s)) => {
                steps = rest;
                &mut Arc::make_mut(s).values[step.bits as usize]
            }
            (_, Value::Array(items)) => {
                let items = Arc::make_mut(items);
                // SAFETY: promised by the caller.
                let i = unsafe { index(*step, rest[0], items.len()) };
                steps = &rest[1..];
                &mut items[i]
            }
            (_, other) => panic!("bad target {other:?}"),
        };
    }
    *target = value;
}
