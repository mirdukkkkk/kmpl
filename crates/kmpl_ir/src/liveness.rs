//! Finds where each register is used for the last time.
//!
//! Arrays and structs are shared until written to, and a write copies them
//! if anything else still holds them. In SSA the old version of an array
//! stays in its register after an update, so without this pass every write
//! in a loop would copy the whole array. Once a register is known to be
//! dead its value can be dropped, or moved instead of shared.

use std::collections::HashSet;

use kmpl_ast::{Ty, Type};

use crate::{Block, BlockId, Function, Module, Op, Reg, Target, Terminator};

/// Fills in `Inst::frees`, `Block::term_frees` and `PathSet::take`.
pub fn annotate(module: &mut Module) {
    for function in &mut module.functions {
        annotate_function(function);
    }
}

/// Gives every branch that passes an array or the like its own block for
/// that edge. A value can be needed on one way out of a branch and passed
/// on along the other. Frees are per terminator, so such a value would stay
/// in its register on the second way, and the next write would copy it.
fn split_edges_passing_values(f: &mut Function) {
    let types = &f.types;
    let shared = |reg: &Reg| {
        !matches!(
            &types[reg.0 as usize],
            Ty::Prim(ty) if !matches!(ty, Type::Str | Type::Bytes | Type::Matrix)
        )
    };

    let first_new = f.blocks.len();
    let mut added = Vec::new();
    for block in &mut f.blocks {
        if !matches!(block.term, Terminator::Branch { .. }) {
            continue;
        }
        for target in block.term.targets_mut() {
            if target.args.iter().any(shared) {
                let edge = Target {
                    block: BlockId((first_new + added.len()) as u32),
                    args: Vec::new(),
                };
                added.push(Block {
                    params: Vec::new(),
                    insts: Vec::new(),
                    term: Terminator::Jump(std::mem::replace(target, edge)),
                    term_frees: Vec::new(),
                });
            }
        }
    }
    f.blocks.extend(added);
}

fn annotate_function(f: &mut Function) {
    split_edges_passing_values(f);
    let live_in = live_in_sets(f);

    for b in 0..f.blocks.len() {
        let mut live = live_out(f, &live_in, b);
        let block = &mut f.blocks[b];

        let mut term_frees = Vec::new();
        block.term.for_each_use(|reg| {
            if live.insert(reg) {
                term_frees.push(reg);
            }
        });
        block.term_frees = term_frees;

        for inst in block.insts.iter_mut().rev() {
            let mut frees = Vec::new();
            if let Some(dest) = inst.dest
                && !live.remove(&dest)
            {
                // Nothing reads the result.
                frees.push(dest);
            }
            inst.op.for_each_use(|reg| {
                if live.insert(reg) {
                    frees.push(reg);
                }
            });

            if let Op::PathSet {
                base, value, take, ..
            } = &mut inst.op
            {
                *take = frees.contains(base) && base != value;
            }
            inst.frees = frees;
        }
    }
}

/// Registers live at the end of block `b`. Block parameters are written by
/// the jump into the block, so they are not live before it.
fn live_out(f: &Function, live_in: &[HashSet<Reg>], b: usize) -> HashSet<Reg> {
    let mut live = HashSet::new();
    for next in f.blocks[b].term.targets() {
        live.extend(live_in[next.block.0 as usize].iter().copied());
    }
    live
}

/// Registers live at the start of each block, by iterating to a fixed point.
fn live_in_sets(f: &Function) -> Vec<HashSet<Reg>> {
    let mut live_in = vec![HashSet::new(); f.blocks.len()];
    let mut changed = true;

    while changed {
        changed = false;
        for b in (0..f.blocks.len()).rev() {
            let mut live = live_out(f, &live_in, b);
            let block = &f.blocks[b];

            block.term.for_each_use(|reg| {
                live.insert(reg);
            });
            for inst in block.insts.iter().rev() {
                if let Some(dest) = inst.dest {
                    live.remove(&dest);
                }
                inst.op.for_each_use(|reg| {
                    live.insert(reg);
                });
            }
            for param in &block.params {
                live.remove(param);
            }

            if live != live_in[b] {
                live_in[b] = live;
                changed = true;
            }
        }
    }
    live_in
}
