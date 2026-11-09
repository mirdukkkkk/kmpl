//! Simplifies the control flow graph: known branches become jumps, block
//! parameters that always get one value disappear, and chains of blocks
//! are joined into one.

use std::collections::HashMap;

use super::remove_block_param;
use crate::{BlockId, Constant, Function, Op, Reg, Terminator};

/// Returns whether anything changed.
pub fn simplify(f: &mut Function) -> bool {
    let mut changed = fold_branches(f);
    changed |= remove_unreachable(f);
    changed |= remove_trivial_params(f);
    changed |= forward_empty_blocks(f);
    changed |= merge_chains(f);
    changed |= remove_unreachable(f);
    changed
}

/// A branch on a known condition, or to the same place either way, is a jump.
fn fold_branches(f: &mut Function) -> bool {
    let known: HashMap<Reg, bool> = f
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter_map(|inst| match (&inst.op, inst.dest) {
            (Op::Const(Constant::Bool(value)), Some(dest)) => Some((dest, *value)),
            _ => None,
        })
        .collect();

    let mut changed = false;
    for block in &mut f.blocks {
        let Terminator::Branch {
            cond,
            then,
            otherwise,
        } = &block.term
        else {
            continue;
        };

        let target = match known.get(cond) {
            Some(true) => then.clone(),
            Some(false) => otherwise.clone(),
            None if then == otherwise => then.clone(),
            None => continue,
        };
        block.term = Terminator::Jump(target);
        changed = true;
    }
    changed
}

fn remove_unreachable(f: &mut Function) -> bool {
    let mut reachable = vec![false; f.blocks.len()];
    let mut stack = vec![0];
    while let Some(b) = stack.pop() {
        if !std::mem::replace(&mut reachable[b], true) {
            stack.extend(
                f.blocks[b]
                    .term
                    .targets()
                    .iter()
                    .map(|t| t.block.0 as usize),
            );
        }
    }
    if reachable.iter().all(|r| *r) {
        return false;
    }

    let mut new_index = vec![0; f.blocks.len()];
    let mut next = 0;
    for (index, is_reachable) in reachable.iter().enumerate() {
        new_index[index] = next;
        if *is_reachable {
            next += 1;
        }
    }

    let mut index = 0;
    f.blocks.retain(|_| {
        index += 1;
        reachable[index - 1]
    });
    for block in &mut f.blocks {
        for target in block.term.targets_mut() {
            target.block = BlockId(new_index[target.block.0 as usize]);
        }
    }
    true
}

/// Replaces every use of `from` with `to`.
fn replace_reg(f: &mut Function, from: Reg, to: Reg) {
    let mut replace = |reg: &mut Reg| {
        if *reg == from {
            *reg = to;
        }
    };
    for block in &mut f.blocks {
        for inst in &mut block.insts {
            inst.op.for_each_use_mut(&mut replace);
        }
        block.term.for_each_use_mut(&mut replace);
    }
}

/// A parameter that only ever receives one value, or itself, is that value.
fn remove_trivial_params(f: &mut Function) -> bool {
    let mut changed = false;
    while let Some((block, position, value)) = find_trivial_param(f) {
        let param = remove_block_param(f, block, position);
        replace_reg(f, param, value);
        changed = true;
    }
    changed
}

fn find_trivial_param(f: &Function) -> Option<(usize, usize, Reg)> {
    let edges: Vec<_> = f
        .blocks
        .iter()
        .flat_map(|block| block.term.targets())
        .collect();

    // The first block's parameters are the function's.
    for (index, block) in f.blocks.iter().enumerate().skip(1) {
        for (position, param) in block.params.iter().enumerate() {
            let mut incoming = edges
                .iter()
                .filter(|target| target.block.0 as usize == index)
                .map(|target| target.args[position])
                .filter(|arg| arg != param);

            if let Some(first) = incoming.next()
                && incoming.all(|arg| arg == first)
            {
                return Some((index, position, first));
            }
        }
    }
    None
}

/// A block that only jumps on is skipped. What it passes along is defined
/// before it, so it is available in everything that jumps to it too.
fn forward_empty_blocks(f: &mut Function) -> bool {
    let mut changed = false;
    for index in 1..f.blocks.len() {
        let block = &f.blocks[index];
        let Terminator::Jump(next) = &block.term else {
            continue;
        };
        if !block.insts.is_empty() || !block.params.is_empty() || next.block.0 as usize == index {
            continue;
        }

        let next = next.clone();
        for source in &mut f.blocks {
            for target in source.term.targets_mut() {
                if target.block.0 as usize == index {
                    *target = next.clone();
                    changed = true;
                }
            }
        }
    }
    changed
}

/// A block whose only predecessor jumps straight to it joins that block.
fn merge_chains(f: &mut Function) -> bool {
    let mut changed = false;
    while let Some((first, second)) = find_chain(f) {
        let tail = std::mem::take(&mut f.blocks[second].insts);
        let term = std::mem::replace(&mut f.blocks[second].term, Terminator::Unreachable);
        f.blocks[first].insts.extend(tail);
        f.blocks[first].term = term;
        changed = true;
    }
    changed
}

fn find_chain(f: &Function) -> Option<(usize, usize)> {
    let predecessors = f.predecessors();

    f.blocks.iter().enumerate().find_map(|(index, block)| {
        let Terminator::Jump(target) = &block.term else {
            return None;
        };
        let next = target.block.0 as usize;
        let joinable = next != index
            && next != 0
            && predecessors[next].len() == 1
            && f.blocks[next].params.is_empty();
        joinable.then_some((index, next))
    })
}
