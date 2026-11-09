//! `std:bits`.

use kmpl_ast::Type;

use crate::{Builtin, ParamType, RetType, Value, ok, ops};

pub(super) static BUILTINS: &[Builtin] = &[Builtin {
    name: "_bits_shr_logical",
    generic: &[],
    params: &[ParamType::Exact(Type::Int), ParamType::Exact(Type::Int)],
    ret: RetType::Exact(Type::Int),
    func: |a, _| {
        let amount = ops::shift_amount(a[1].as_int())?;
        ok(Value::Int(((a[0].as_int() as u64) >> amount) as i64))
    },
}];
