//! `std:text`.

use kmpl_ast::Type;

use crate::{Builtin, ParamType, RetType, Value, ok, pair};

pub(super) static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "_text_parse_int",
        generic: &[],
        params: &[ParamType::Exact(Type::Str)],
        ret: RetType::Pair(Type::Int, Type::Bool),
        func: |a, _| {
            let parsed = a[0].as_str().trim().parse::<i64>().ok();
            ok(pair(
                Value::Int(parsed.unwrap_or(0)),
                Value::Bool(parsed.is_some()),
            ))
        },
    },
    Builtin {
        name: "_text_parse_real",
        generic: &[],
        params: &[ParamType::Exact(Type::Str)],
        ret: RetType::Pair(Type::Real, Type::Bool),
        func: |a, _| {
            // Not `inf` or `NaN`, and not `1e400`, like literals in a program.
            let parsed = a[0]
                .as_str()
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite());
            ok(pair(
                Value::Real(parsed.unwrap_or(0.0)),
                Value::Bool(parsed.is_some()),
            ))
        },
    },
];
