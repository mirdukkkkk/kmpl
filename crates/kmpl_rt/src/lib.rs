//! The runtime linked into every program made by `kmpl build`.
//!
//! The generated code calls these functions through the C calling
//! convention and hands over raw pointers, so this crate needs `unsafe`.
#![allow(unsafe_code)]

mod operations;

use std::cell::RefCell;
use std::io::{BufReader, BufWriter, Stdin, Stdout, Write};
use std::sync::Arc;
use std::time::Instant;

use kmpl_builtins::abi::{self, ArrayView, MatrixShape, Slot};
use kmpl_builtins::{Builtin, Io, Random, StructLayout, Value, ops};

use operations::{operation, to_slot, value};

/// What `print`, `read_line` and `args` work with.
pub(crate) struct State {
    out: BufWriter<Stdout>,
    input: BufReader<Stdin>,
    args: Vec<String>,
    random: Random,
    started: Instant,
    /// Reused for the arguments of built-ins.
    scratch: Vec<Value>,
    pub(crate) layouts: Vec<Arc<StructLayout>>,
    /// By position, as generated code refers to them.
    builtins: Vec<&'static Builtin>,
}

thread_local! {
    pub(crate) static STATE: RefCell<State> = RefCell::new(State {
        out: BufWriter::new(std::io::stdout()),
        input: BufReader::new(std::io::stdin()),
        args: std::env::args_os()
            .skip(1)
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
        random: Random::default(),
        started: Instant::now(),
        scratch: Vec::new(),
        layouts: Vec::new(),
        builtins: kmpl_builtins::all().collect(),
    });
}

/// Writes out what was printed. Before an error, so it appears before it.
fn flush(state: &mut State) {
    if let Err(e) = state.out.flush() {
        eprintln!("kmpl: cannot write output: {e}");
        std::process::exit(1);
    }
}

fn stop(state: &mut State, message: &str, location: &str) -> ! {
    flush(state);
    eprint!("error: {message}\n{location}");
    std::process::exit(1);
}

/// Stops the program with a runtime error.
pub(crate) fn fail(message: &str, location: &str) -> ! {
    STATE.with_borrow_mut(|state| stop(state, message, location))
}

/// # Safety
/// `pointer` and `length` describe readable memory: a constant the code
/// generator put into the program.
pub(crate) unsafe fn text(pointer: *const u8, length: u64) -> String {
    // SAFETY: promised by the caller.
    let bytes = unsafe { std::slice::from_raw_parts(pointer, length as usize) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Receives the struct layouts: one line each, the name and then the fields.
///
/// # Safety
/// `layouts` and `length` describe a constant, see `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_init(layouts: *const u8, length: u64) {
    // SAFETY: promised by the caller.
    let layouts = unsafe { text(layouts, length) };
    let layouts = layouts
        .lines()
        .map(|line| {
            let mut words = line.split(' ').map(str::to_owned);
            Arc::new(StructLayout {
                name: words.next().unwrap_or_default(),
                fields: words.collect(),
            })
        })
        .collect();
    STATE.with_borrow_mut(|state| state.layouts = layouts);
}

/// Stops the program with a runtime error.
///
/// # Safety
/// Both pairs describe constants, see `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_error(
    message: *const u8,
    message_length: u64,
    location: *const u8,
    location_length: u64,
) -> ! {
    // SAFETY: promised by the caller.
    let (message, location) = unsafe {
        (
            text(message, message_length),
            text(location, location_length),
        )
    };
    fail(&message, &location)
}

/// # Safety
/// `value` is a valid pointer to a boxed value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_clone(value: *const Value) -> *mut Value {
    // SAFETY: promised by the caller.
    Box::into_raw(Box::new(unsafe { (*value).clone() }))
}

/// # Safety
/// `value` came from this runtime and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_drop(value: *mut Value) {
    // SAFETY: promised by the caller.
    drop(unsafe { Box::from_raw(value) });
}

/// Packs a vector register into a `Value`. Unused components are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn kmpl_rt_box_vector(kind: u64, a: f64, b: f64, c: f64, d: f64) -> *mut Value {
    Box::into_raw(Box::new(match kind {
        abi::KIND_COMPLEX => Value::Complex { re: a, im: b },
        abi::KIND_VEC2 => Value::Vec2([a, b]),
        abi::KIND_VEC3 => Value::Vec3([a, b, c]),
        _ => Value::Vec4([a, b, c, d]),
    }))
}

/// Writes the components to `out`, and releases the `Value`.
///
/// # Safety
/// `value` came from this runtime and is not used again, and `out` has room
/// for four `f64`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_unbox_vector(value: *mut Value, out: *mut f64) {
    // SAFETY: promised by the caller.
    unsafe {
        let value = *Box::from_raw(value);
        let parts = match &value {
            Value::Complex { re, im } => &[*re, *im][..],
            other => other.components(),
        };
        std::ptr::copy_nonoverlapping(parts.as_ptr(), out, parts.len());
    }
}

/// Where the numbers of a flat array or a matrix are, for reading.
///
/// # Safety
/// `value` is a valid pointer to a `[real]`, an `[int]` or a `matrix`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_array_view(value: *const Value) -> ArrayView {
    // SAFETY: promised by the caller.
    match unsafe { &*value } {
        Value::Reals(items) => ArrayView {
            data: items.as_ptr() as *mut u8,
            len: items.len() as u64,
        },
        Value::Ints(items) => ArrayView {
            data: items.as_ptr() as *mut u8,
            len: items.len() as u64,
        },
        Value::Matrix(matrix) => ArrayView {
            data: matrix.data.as_ptr() as *mut u8,
            len: matrix.data.len() as u64,
        },
        other => panic!("expected a flat array, got {other:?}"),
    }
}

/// How many rows and columns a matrix has.
///
/// # Safety
/// `value` is a valid pointer to a `matrix`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_matrix_shape(value: *const Value) -> MatrixShape {
    // SAFETY: promised by the caller.
    let matrix = unsafe { &*value }.as_matrix();
    MatrixShape {
        rows: matrix.rows as u64,
        cols: matrix.cols as u64,
    }
}

/// Makes sure nothing else shares this flat array or matrix, copying it if
/// something does, and says where its numbers are for writing.
///
/// # Safety
/// `value` is a valid pointer to a `[real]`, an `[int]` or a `matrix`, owned
/// by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_array_unique(value: *mut Value) -> ArrayView {
    // SAFETY: promised by the caller.
    match unsafe { &mut *value } {
        Value::Reals(items) => {
            let items = Arc::make_mut(items);
            ArrayView {
                data: items.as_mut_ptr() as *mut u8,
                len: items.len() as u64,
            }
        }
        Value::Ints(items) => {
            let items = Arc::make_mut(items);
            ArrayView {
                data: items.as_mut_ptr() as *mut u8,
                len: items.len() as u64,
            }
        }
        Value::Matrix(matrix) => {
            let data = &mut Arc::make_mut(matrix).data;
            ArrayView {
                data: data.as_mut_ptr() as *mut u8,
                len: data.len() as u64,
            }
        }
        other => panic!("expected a flat array, got {other:?}"),
    }
}

/// Stops the program with the error for a bad index.
///
/// # Safety
/// The location is a constant, see `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_index_error(
    index: i64,
    len: u64,
    location: *const u8,
    location_length: u64,
) -> ! {
    let message = ops::checked_index(index, len as usize)
        .expect_err("generated code checked that this index is bad");
    // SAFETY: promised by the caller.
    fail(&message, &unsafe { text(location, location_length) })
}

/// Stops the program with the error for a bad row of a matrix, or a bad
/// column if `column` is not zero.
///
/// # Safety
/// The location is a constant, see `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_cell_error(
    index: i64,
    count: u64,
    column: u64,
    location: *const u8,
    location_length: u64,
) -> ! {
    let what = if column == 0 { "row" } else { "column" };
    let message = ops::checked_line(index, count as usize, what)
        .expect_err("generated code checked that this row or column is bad");
    // SAFETY: promised by the caller.
    fail(&message, &unsafe { text(location, location_length) })
}

/// Runs operation `code` of `abi`.
///
/// # Safety
/// `args` points to `count` slots as that operation expects, `result` to
/// writable memory for one, and the location is a constant, see `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_op(
    code: u64,
    extra: u64,
    args: *const Slot,
    count: u64,
    result: *mut Slot,
    location: *const u8,
    location_length: u64,
) {
    // SAFETY: promised by the caller.
    unsafe {
        let args = std::slice::from_raw_parts(args, count as usize);
        match operation(code, extra, args) {
            Ok(value) => result.write(to_slot(value)),
            Err(message) => fail(&message, &text(location, location_length)),
        }
    }
}

/// Calls built-in number `id`. A null `result` drops what it returns.
///
/// # Safety
/// `args` points to `count` valid slots, `result` is null or points to
/// writable memory for one, and the location is a constant, see `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kmpl_rt_builtin(
    id: u64,
    args: *const Slot,
    count: u64,
    result: *mut Slot,
    location: *const u8,
    location_length: u64,
) {
    // SAFETY: promised by the caller.
    let args = unsafe { std::slice::from_raw_parts(args, count as usize) };

    STATE.with_borrow_mut(|state| {
        let mut values = std::mem::take(&mut state.scratch);
        values.clear();
        // SAFETY: promised by the caller.
        values.extend(args.iter().map(|slot| unsafe { value(*slot) }));

        let mut io = Io {
            out: &mut state.out,
            input: &mut state.input,
            args: &state.args,
            random: &mut state.random,
            started: state.started,
        };
        let outcome = (state.builtins[id as usize].func)(&mut values, &mut io);
        // Holding on to the arguments would make a later write to one of
        // those arrays copy it.
        values.clear();
        state.scratch = values;

        match outcome {
            // SAFETY: promised by the caller.
            Ok(Some(value)) if !result.is_null() => unsafe { result.write(to_slot(value)) },
            Ok(_) => {}
            Err(message) => {
                // SAFETY: promised by the caller.
                let location = unsafe { text(location, location_length) };
                stop(state, &message, &location);
            }
        }
    });
}

/// `main` would clash with the one the test harness generates.
#[cfg(not(test))]
mod entry {
    use super::{STATE, flush};

    /// Dropping or printing deeply nested data recurses, like in the `kmpl`
    /// binary. Only touched pages are actually allocated.
    const STACK_SIZE: usize = 1 << 30;

    unsafe extern "C" {
        /// The compiled program.
        fn kmpl_program();
    }

    fn run() {
        // SAFETY: the code generator defines `kmpl_program` with this signature.
        unsafe { kmpl_program() };
        STATE.with_borrow_mut(flush);
    }

    #[unsafe(no_mangle)]
    extern "C" fn main() -> std::ffi::c_int {
        let worker = std::thread::Builder::new()
            .name("kmpl".to_owned())
            .stack_size(STACK_SIZE)
            .spawn(run)
            .expect("cannot start the main thread");
        match worker.join() {
            Ok(()) => 0,
            Err(_) => 101,
        }
    }
}
