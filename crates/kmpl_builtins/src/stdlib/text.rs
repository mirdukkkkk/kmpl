//! `std:text`.

use kmpl_ast::Type;

use std::sync::Arc;

use crate::{Builtin, ParamType, RetType, Value, ok, optional};

const TEXT: ParamType = ParamType::Exact(Type::Str);

fn text(value: &str) -> Value {
    Value::Str(value.into())
}

/// Is there at least one character, and does every one pass the test?
fn every_char(value: &Value, test: fn(char) -> bool) -> Value {
    let s = value.as_str();
    Value::Bool(!s.is_empty() && s.chars().all(test))
}

pub(super) static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "_text_split",
        generic: &[],
        params: &[TEXT, TEXT],
        ret: RetType::ArrayOf(Type::Str),
        func: |a, _| {
            let separator = a[1].as_str();
            if separator.is_empty() {
                return Err("`split` needs a separator that is not empty".to_owned());
            }
            let parts = a[0].as_str().split(separator).map(text).collect();
            ok(Value::Array(Arc::new(parts)))
        },
    },
    Builtin {
        name: "_text_join",
        generic: &[],
        params: &[ParamType::ArrayOfElem, TEXT],
        ret: RetType::Exact(Type::Str),
        func: |a, _| {
            let parts: Vec<&str> = a[0].as_array().iter().map(Value::as_str).collect();
            ok(text(&parts.join(a[1].as_str())))
        },
    },
    Builtin {
        name: "_text_trim",
        generic: &[],
        params: &[TEXT],
        ret: RetType::Exact(Type::Str),
        func: |a, _| ok(text(a[0].as_str().trim())),
    },
    Builtin {
        name: "_text_starts_with",
        generic: &[],
        params: &[TEXT, TEXT],
        ret: RetType::Exact(Type::Bool),
        func: |a, _| ok(Value::Bool(a[0].as_str().starts_with(a[1].as_str()))),
    },
    Builtin {
        name: "_text_ends_with",
        generic: &[],
        params: &[TEXT, TEXT],
        ret: RetType::Exact(Type::Bool),
        func: |a, _| ok(Value::Bool(a[0].as_str().ends_with(a[1].as_str()))),
    },
    Builtin {
        name: "_text_contains",
        generic: &[],
        params: &[TEXT, TEXT],
        ret: RetType::Exact(Type::Bool),
        func: |a, _| ok(Value::Bool(a[0].as_str().contains(a[1].as_str()))),
    },
    Builtin {
        name: "_text_find",
        generic: &[],
        params: &[TEXT, TEXT],
        ret: RetType::Optional(Type::Int),
        func: |a, _| {
            let haystack = a[0].as_str();
            let position = haystack
                .find(a[1].as_str())
                .map(|byte| Value::Int(haystack[..byte].chars().count() as i64));
            ok(optional(position))
        },
    },
    Builtin {
        name: "_text_replace",
        generic: &[],
        params: &[TEXT, TEXT, TEXT],
        ret: RetType::Exact(Type::Str),
        func: |a, _| {
            let from = a[1].as_str();
            if from.is_empty() {
                return Err("`replace` needs something to look for".to_owned());
            }
            ok(text(&a[0].as_str().replace(from, a[2].as_str())))
        },
    },
    Builtin {
        name: "_text_repeat",
        generic: &[],
        params: &[TEXT, ParamType::Exact(Type::Int)],
        ret: RetType::Exact(Type::Str),
        func: |a, _| {
            let (s, times) = (a[0].as_str(), a[1].as_int());
            let too_long = || format!("repeating this text {times} times is too long");
            let count = usize::try_from(times)
                .map_err(|_| format!("cannot repeat a text {times} times"))?;
            let total = s.len().checked_mul(count).ok_or_else(too_long)?;
            let mut check = String::new();
            check.try_reserve_exact(total).map_err(|_| too_long())?;
            drop(check);
            ok(text(&s.repeat(count)))
        },
    },
    Builtin {
        name: "_text_to_upper",
        generic: &[],
        params: &[TEXT],
        ret: RetType::Exact(Type::Str),
        func: |a, _| ok(text(&a[0].as_str().to_uppercase())),
    },
    Builtin {
        name: "_text_to_lower",
        generic: &[],
        params: &[TEXT],
        ret: RetType::Exact(Type::Str),
        func: |a, _| ok(text(&a[0].as_str().to_lowercase())),
    },
    Builtin {
        name: "_text_is_digit",
        generic: &[],
        params: &[TEXT],
        ret: RetType::Exact(Type::Bool),
        func: |a, _| ok(every_char(&a[0], |c| c.is_ascii_digit())),
    },
    Builtin {
        name: "_text_is_letter",
        generic: &[],
        params: &[TEXT],
        ret: RetType::Exact(Type::Bool),
        func: |a, _| ok(every_char(&a[0], char::is_alphabetic)),
    },
    Builtin {
        name: "_text_is_space",
        generic: &[],
        params: &[TEXT],
        ret: RetType::Exact(Type::Bool),
        func: |a, _| ok(every_char(&a[0], char::is_whitespace)),
    },
    Builtin {
        name: "_text_code",
        generic: &[],
        params: &[TEXT],
        ret: RetType::Exact(Type::Int),
        func: |a, _| {
            let mut chars = a[0].as_str().chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => ok(Value::Int(i64::from(u32::from(c)))),
                _ => Err(format!(
                    "`code` needs exactly one character, got \"{}\"",
                    a[0].as_str()
                )),
            }
        },
    },
    Builtin {
        name: "_text_from_code",
        generic: &[],
        params: &[ParamType::Exact(Type::Int)],
        ret: RetType::Optional(Type::Str),
        func: |a, _| {
            let c = u32::try_from(a[0].as_int()).ok().and_then(char::from_u32);
            ok(optional(c.map(|c| text(&c.to_string()))))
        },
    },
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
