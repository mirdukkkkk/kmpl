//! `std:fs`.

use std::sync::Arc;

use kmpl_ast::Type;

use crate::{Builtin, ParamType, RetType, Value, ok, pair};

const STRING: ParamType = ParamType::Exact(Type::Str);

fn text(value: String) -> Value {
    Value::Str(value.into())
}

fn no_error() -> Value {
    text(String::new())
}

fn describe(path: &str, error: std::io::Error) -> Value {
    text(format!("{path}: {error}"))
}

pub(super) static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "_fs_read",
        generic: &[],
        params: &[STRING],
        ret: RetType::Pair(Type::Bytes, Type::Str),
        func: |a, _| {
            let path = a[0].as_str();
            ok(match std::fs::read(path) {
                Ok(data) => pair(Value::Bytes(Arc::new(data)), no_error()),
                Err(e) => pair(Value::Bytes(Arc::default()), describe(path, e)),
            })
        },
    },
    Builtin {
        name: "_fs_read_text",
        generic: &[],
        params: &[STRING],
        ret: RetType::Pair(Type::Str, Type::Str),
        func: |a, _| {
            let path = a[0].as_str();
            ok(match std::fs::read_to_string(path) {
                Ok(content) => pair(text(content), no_error()),
                Err(e) => pair(no_error(), describe(path, e)),
            })
        },
    },
    Builtin {
        name: "_fs_write",
        generic: &[],
        params: &[STRING, ParamType::Exact(Type::Bytes)],
        ret: RetType::Exact(Type::Str),
        func: |a, _| {
            let path = a[0].as_str();
            ok(match std::fs::write(path, a[1].as_bytes()) {
                Ok(()) => no_error(),
                Err(e) => describe(path, e),
            })
        },
    },
];
