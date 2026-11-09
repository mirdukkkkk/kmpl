//! The runtime linked into every program made by `kmpl build`.
//!
//! The generated code calls these functions through the C calling
//! convention and hands over raw pointers, so this crate needs `unsafe`.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::io::{BufReader, BufWriter, Stdin, Stdout, Write};

use kmpl_builtins::abi::{self, Slot};
use kmpl_builtins::{BUILTINS, Io, Value};

/// What `print`, `read_line` and `args` work with.
struct State {
    out: BufWriter<Stdout>,
    input: BufReader<Stdin>,
    args: Vec<String>,
    /// Reused for the arguments of built-ins.
    scratch: Vec<Value>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        out: BufWriter::new(std::io::stdout()),
        input: BufReader::new(std::io::stdin()),
        args: std::env::args_os()
            .skip(1)
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
        scratch: Vec::new(),
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

/// # Safety
/// `pointer` and `length` describe readable memory: a constant the code
/// generator put into the program.
unsafe fn text(pointer: *const u8, length: u64) -> String {
    // SAFETY: promised by the caller.
    let bytes = unsafe { std::slice::from_raw_parts(pointer, length as usize) };
    String::from_utf8_lossy(bytes).into_owned()
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
    STATE.with_borrow_mut(|state| stop(state, &message, &location))
}

fn from_slot(slot: Slot) -> Value {
    match slot.tag {
        abi::TAG_INT => Value::Int(slot.bits as i64),
        abi::TAG_REAL => Value::Real(f64::from_bits(slot.bits)),
        abi::TAG_BOOL => Value::Bool(slot.bits != 0),
        tag => panic!("bad slot tag {tag}"),
    }
}

fn to_slot(value: Value) -> Slot {
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
        other => panic!("cannot pass {other:?} yet"),
    }
}

/// Calls entry `id` of `BUILTINS`.
///
/// # Safety
/// `args` points to `count` slots, `result` to writable memory for one, and
/// the location is a constant, see `text`.
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
        values.extend(args.iter().copied().map(from_slot));

        let mut io = Io {
            out: &mut state.out,
            input: &mut state.input,
            args: &state.args,
        };
        let outcome = (BUILTINS[id as usize].func)(&mut values, &mut io);
        values.clear();
        state.scratch = values;

        match outcome {
            // SAFETY: promised by the caller.
            Ok(Some(value)) => unsafe { result.write(to_slot(value)) },
            Ok(None) => {}
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
