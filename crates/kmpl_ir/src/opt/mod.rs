//! Optimizations on the IR. Run before `annotate`.
//!
//! None of them may change what a program does. In particular:
//! - an instruction that can stop the program with an error (`1 / 0`, `int`
//!   overflow, a bad index) stays, even if its result is unused;
//! - calls and built-ins keep their order;
//! - `real` arithmetic is never rearranged: floats are not associative.

mod cfg;
mod compact;
mod dce;
mod fold;
mod licm;
mod range;

use crate::{Function, Module};

/// Rounds of the passes that enable each other, at most.
const MAX_ROUNDS: usize = 16;

pub fn optimize(module: &mut Module) {
    for function in &mut module.functions {
        optimize_function(function);
    }
}

fn optimize_function(f: &mut Function) {
    range::simplify(f);
    for _ in 0..MAX_ROUNDS {
        let mut changed = fold::fold(f);
        changed |= cfg::simplify(f);
        changed |= dce::eliminate(f);
        if !changed {
            break;
        }
    }
    if licm::hoist(f) {
        // Moving work out can leave empty blocks behind.
        cfg::simplify(f);
    }
    compact::compact(f);
}
