//! What generated code asks the runtime to do with boxed values.

use std::sync::Arc;

use kmpl_builtins::abi::{self, Slot};
use kmpl_builtins::{EnumValue, StructValue, Value, ops};

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
            abi::OP_MAKE_ARRAY => match extra {
                abi::ARRAY_OF_REALS => Value::Reals(Arc::new(
                    args.iter().map(|s| f64::from_bits(s.bits)).collect(),
                )),
                abi::ARRAY_OF_INTS => Value::Ints(Arc::new(args.iter().map(|s| int(*s)).collect())),
                _ => Value::Array(Arc::new(values(args))),
            },
            abi::OP_ARRAY_FILL => ops::array_fill(value(args[0]), int(args[1]))?,
            abi::OP_ARRAY_LEN => Value::Int(borrowed(args[0]).array_len() as i64),
            abi::OP_INDEX => {
                let array = borrowed(args[0]);
                let position = ops::checked_index(int(args[1]), array.array_len())?;
                element(array, position)
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
            abi::OP_MAKE_ENUM => Value::Enum(Arc::new(EnumValue {
                layout: STATE.with_borrow(|state| state.layouts[(extra >> 32) as usize].clone()),
                variant: (extra & 0xffff_ffff) as usize,
                values: values(args),
            })),
            abi::OP_MAKE_SOME => Value::Optional(Some(Arc::new(value(args[0])))),
            abi::OP_MAKE_NONE => Value::Optional(None),
            abi::OP_VARIANT_OF => Value::Int(match borrowed(args[0]) {
                Value::Enum(e) => e.variant as i64,
                Value::Optional(None) => 0,
                Value::Optional(Some(_)) => 1,
                other => panic!("the variant of {other:?}"),
            }),
            abi::OP_VARIANT_FIELD => match borrowed(args[0]) {
                Value::Enum(e) => e.values[extra as usize].clone(),
                Value::Optional(Some(inner)) => (**inner).clone(),
                other => panic!("a value inside {other:?}"),
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
    let array = unsafe { borrowed(args[0]) };
    let len = array.array_len();
    let mut bounds = args[1..].iter();
    let mut next_if = |flag| (flags & flag != 0).then(|| int(*bounds.next().expect("passed")));

    let from = next_if(abi::SLICE_HAS_START).unwrap_or(0);
    let (to, inclusive) = match next_if(abi::SLICE_HAS_END) {
        Some(end) => (end, flags & abi::SLICE_INCLUSIVE != 0),
        None => (len as i64, false),
    };
    let range = ops::slice_bounds(from, to, inclusive, len)?;
    Ok(match array {
        Value::Str(text) => {
            let part: String = text.chars().skip(range.start).take(range.len()).collect();
            Value::Str(part.into())
        }
        Value::Reals(items) => Value::Reals(Arc::new(items[range].to_vec())),
        Value::Ints(items) => Value::Ints(Arc::new(items[range].to_vec())),
        Value::Bytes(items) => Value::Bytes(Arc::new(items[range].to_vec())),
        other => Value::Array(Arc::new(other.as_array()[range].to_vec())),
    })
}

/// The element at a position already checked to be inside `array`.
fn element(array: &Value, position: usize) -> Value {
    match array {
        Value::Str(text) => {
            let c = text.chars().nth(position).expect("checked to be inside");
            Value::Str(c.to_string().into())
        }
        Value::Reals(items) => Value::Real(items[position]),
        Value::Ints(items) => Value::Int(items[position]),
        Value::Bytes(items) => Value::Int(i64::from(items[position])),
        other => other.as_array()[position].clone(),
    }
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

/// The position of `[row, col]` in the numbers of a matrix. `steps` starts
/// with the row, where it is reported, the column and where that is.
///
/// # Safety
/// See `slot_text`.
unsafe fn cell(steps: &[Slot], rows: usize, cols: usize) -> usize {
    let line = |i: Slot, location: Slot, count, what| {
        ops::checked_line(int(i), count, what).unwrap_or_else(|message| {
            // SAFETY: promised by the caller.
            fail(&message, &unsafe { slot_text(location) })
        })
    };
    let row = line(steps[0], steps[1], rows, "row");
    let col = line(steps[2], steps[3], cols, "column");
    row * cols + col
}

/// # Safety
/// See `operation`.
unsafe fn read_path(root: &Value, mut steps: &[Slot]) -> Value {
    let mut target = root;
    while let [step, rest @ ..] = steps {
        match (step.tag, target) {
            (abi::TAG_FIELD, Value::Struct(s)) => {
                steps = rest;
                target = &s.values[step.bits as usize];
            }
            (_, Value::Array(items)) => {
                // SAFETY: promised by the caller.
                let i = unsafe { index(*step, rest[0], items.len()) };
                steps = &rest[1..];
                target = &items[i];
            }
            // Numbers have no parts, so this is the last step.
            (_, array @ (Value::Reals(_) | Value::Ints(_) | Value::Bytes(_))) => {
                // SAFETY: promised by the caller.
                let i = unsafe { index(*step, rest[0], array.array_len()) };
                return element(array, i);
            }
            (_, Value::Matrix(matrix)) => {
                // SAFETY: promised by the caller.
                let position = unsafe { cell(steps, matrix.rows, matrix.cols) };
                return Value::Real(matrix.data[position]);
            }
            (_, other) => panic!("bad target {other:?}"),
        }
    }
    target.clone()
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
            // Numbers have no parts, so this is the last step.
            (_, Value::Reals(items)) => {
                let items = Arc::make_mut(items);
                // SAFETY: promised by the caller.
                let i = unsafe { index(*step, rest[0], items.len()) };
                items[i] = value.as_real();
                return;
            }
            (_, Value::Ints(items)) => {
                let items = Arc::make_mut(items);
                // SAFETY: promised by the caller.
                let i = unsafe { index(*step, rest[0], items.len()) };
                items[i] = value.as_int();
                return;
            }
            (_, Value::Matrix(matrix)) => {
                let matrix = Arc::make_mut(matrix);
                // SAFETY: promised by the caller.
                let position = unsafe { cell(steps, matrix.rows, matrix.cols) };
                matrix.data[position] = value.as_real();
                return;
            }
            (_, Value::Bytes(items)) => {
                let items = Arc::make_mut(items);
                // SAFETY: promised by the caller.
                let i = unsafe { index(*step, rest[0], items.len()) };
                items[i] = ops::byte(value.as_int()).unwrap_or_else(|message| {
                    // SAFETY: promised by the caller.
                    fail(&message, &unsafe { slot_text(rest[0]) })
                });
                return;
            }
            (_, other) => panic!("bad target {other:?}"),
        };
    }
    *target = value;
}
