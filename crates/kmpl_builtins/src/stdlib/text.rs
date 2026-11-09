//! `std:text`.

use kmpl_ast::Type;

use crate::{Builtin, ParamType, RetType, Value, ok, optional};

pub(super) static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "_text_parse_int",
        generic: &[],
        params: &[ParamType::Exact(Type::Str)],
        ret: RetType::Optional(Type::Int),
        func: |a, _| {
            let parsed = a[0].as_str().trim().parse::<i64>().ok();
            ok(optional(parsed.map(Value::Int)))
        },
    },
    Builtin {
        name: "_text_parse_real",
        generic: &[],
        params: &[ParamType::Exact(Type::Str)],
        ret: RetType::Optional(Type::Real),
        func: |a, _| {
            // Not `inf` or `NaN`, and not `1e400`, like literals in a program.
            let parsed = a[0]
                .as_str()
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite());
            ok(optional(parsed.map(Value::Real)))
        },
    },
];
