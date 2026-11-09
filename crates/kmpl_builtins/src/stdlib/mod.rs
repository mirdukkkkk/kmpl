//! The functions behind `std:` libraries. Their names start with `_`, and
//! only the libraries themselves may call them. The libraries are written
//! in kmpl, in `crates/kmpl/std/`.

mod bits;
mod fs;
mod io;
mod linalg;
mod random;
mod stats;
mod text;
mod time;

pub use random::Random;

use crate::Builtin;

pub(crate) static GROUPS: [&[Builtin]; 8] = [
    linalg::BUILTINS,
    stats::BUILTINS,
    fs::BUILTINS,
    io::BUILTINS,
    text::BUILTINS,
    bits::BUILTINS,
    random::BUILTINS,
    time::BUILTINS,
];
