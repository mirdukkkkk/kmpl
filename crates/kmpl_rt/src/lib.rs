//! The runtime linked into every program made by `kmpl build`.
//!
//! The generated code calls these functions through the C calling
//! convention and hands over raw pointers, so this crate needs `unsafe`.
#![allow(unsafe_code)]

/// `main` would clash with the one the test harness generates.
#[cfg(not(test))]
mod entry {
    /// Dropping or printing deeply nested data recurses, like in the `kmpl`
    /// binary. Only touched pages are actually allocated.
    const STACK_SIZE: usize = 1 << 30;

    unsafe extern "C" {
        /// The compiled program.
        fn kmpl_program();
    }

    #[unsafe(no_mangle)]
    extern "C" fn main() -> std::ffi::c_int {
        // SAFETY: the code generator defines `kmpl_program` with this signature.
        let program = || unsafe { kmpl_program() };
        let worker = std::thread::Builder::new()
            .name("kmpl".to_owned())
            .stack_size(STACK_SIZE)
            .spawn(program)
            .expect("cannot start the main thread");
        match worker.join() {
            Ok(()) => 0,
            Err(_) => 101,
        }
    }
}
