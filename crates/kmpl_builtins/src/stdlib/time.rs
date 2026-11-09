//! `std:time`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kmpl_ast::Type;

use crate::{Builtin, ParamType, RetType, Value, ok};

pub(super) static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "_time_now",
        generic: &[],
        params: &[],
        ret: RetType::Exact(Type::Real),
        func: |_, _| {
            let since = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "the system clock is set before 1970".to_owned())?;
            ok(Value::Real(since.as_secs_f64()))
        },
    },
    Builtin {
        name: "_time_clock",
        generic: &[],
        params: &[],
        ret: RetType::Exact(Type::Real),
        func: |_, io| ok(Value::Real(io.started.elapsed().as_secs_f64())),
    },
    Builtin {
        name: "_time_sleep",
        generic: &[],
        params: &[ParamType::Exact(Type::Real)],
        ret: RetType::Nothing,
        func: |a, io| {
            let seconds = a[0].as_real();
            let pause = Duration::try_from_secs_f64(seconds)
                .map_err(|_| format!("cannot sleep for {seconds:?} seconds"))?;
            // What was printed before the pause has to show during it.
            io.out
                .flush()
                .map_err(|e| format!("cannot write output: {e}"))?;
            std::thread::sleep(pause);
            Ok(None)
        },
    },
];
