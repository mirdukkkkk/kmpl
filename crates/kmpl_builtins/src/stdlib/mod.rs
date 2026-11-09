//! The functions behind `std:` libraries. Their names start with `_`, and
//! only the libraries themselves may call them. The libraries are written
//! in kmpl, in `crates/kmpl/std/`.

mod bits;
mod fs;
mod io;
mod text;

use crate::Builtin;

pub(crate) static GROUPS: [&[Builtin]; 4] =
    [fs::BUILTINS, io::BUILTINS, text::BUILTINS, bits::BUILTINS];
