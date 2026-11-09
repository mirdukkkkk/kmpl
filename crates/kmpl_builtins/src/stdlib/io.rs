//! `std:io`.

use std::sync::Arc;

use kmpl_ast::Type;

use crate::{Builtin, RetType, Value, ok, optional};

pub(super) static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "_io_args",
        generic: &[],
        params: &[],
        ret: RetType::ArrayOf(Type::Str),
        func: |_, io| {
            let items = io
                .args
                .iter()
                .map(|a| Value::Str(a.as_str().into()))
                .collect();
            ok(Value::Array(Arc::new(items)))
        },
    },
    Builtin {
        name: "_io_read_line",
        generic: &[],
        params: &[],
        ret: RetType::Optional(Type::Str),
        func: |_, io| {
            // A prompt printed just before has to show while this waits.
            io.out
                .flush()
                .map_err(|e| format!("cannot write output: {e}"))?;

            let mut line = String::new();
            let read = io
                .input
                .read_line(&mut line)
                .map_err(|e| format!("cannot read input: {e}"))?;

            let text = line.strip_suffix('\n').unwrap_or(&line);
            let text = text.strip_suffix('\r').unwrap_or(text);
            ok(optional((read > 0).then(|| Value::Str(text.into()))))
        },
    },
];
