//! Moves work out of loops: an instruction whose inputs do not change inside
//! a loop only has to run once, before it.
//!
//! Only instructions that cannot fail are moved, because the loop may not
//! run at all. Only numbers and strings are moved: a moved array would stay
//! alive for the whole loop, so writes to it would have to copy it.

use std::collections::{HashMap, HashSet};

use kmpl_ast::Ty;

use super::dce::is_pure;
use crate::{Block, BlockId, Function, Reg, Target, Terminator};

struct Loop {
    header: usize,
    body: HashSet<usize>,
}

/// Returns whether anything changed.
pub fn hoist(f: &mut Function) -> bool {
    add_preheaders(f);
    let mut loops = find_loops(f);
    // Inner loops first, so work can move out through several levels.
    loops.sort_by_key(|l| l.body.len());

    let mut changed = false;
    for found in &loops {
        if let Some(preheader) = preheader(f, found) {
            changed |= hoist_from(f, found, preheader);
        }
    }
    changed
}

/// Which blocks dominate each block: every path to it goes through them.
fn dominators(f: &Function) -> Vec<Vec<bool>> {
    let n = f.blocks.len();
    let predecessors = f.predecessors();

    let mut dom = vec![vec![true; n]; n];
    dom[0] = vec![false; n];
    dom[0][0] = true;

    let mut changed = true;
    while changed {
        changed = false;
        for b in 1..n {
            let mut new = vec![true; n];
            for &p in &predecessors[b] {
                for (bit, from_p) in new.iter_mut().zip(&dom[p]) {
                    *bit &= *from_p;
                }
            }
            new[b] = true;
            if new != dom[b] {
                dom[b] = new;
                changed = true;
            }
        }
    }
    dom
}

/// Natural loops: a jump back to a block that dominates the jump. Loops
/// with the same header are one loop (`continue` makes several back edges).
fn find_loops(f: &Function) -> Vec<Loop> {
    let dom = dominators(f);
    let predecessors = f.predecessors();

    let mut bodies: HashMap<usize, HashSet<usize>> = HashMap::new();
    for (tail, block) in f.blocks.iter().enumerate() {
        for target in block.term.targets() {
            let header = target.block.0 as usize;
            if !dom[tail][header] {
                continue;
            }

            let body = bodies
                .entry(header)
                .or_insert_with(|| HashSet::from([header]));
            let mut stack = vec![tail];
            while let Some(b) = stack.pop() {
                if body.insert(b) {
                    stack.extend(&predecessors[b]);
                }
            }
        }
    }

    bodies
        .into_iter()
        .map(|(header, body)| Loop { header, body })
        .collect()
}

/// The jumps into `found` from outside it, as (block, position of target).
fn outside_edges(f: &Function, found: &Loop) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    for (index, block) in f.blocks.iter().enumerate() {
        if found.body.contains(&index) {
            continue;
        }
        for (position, target) in block.term.targets().iter().enumerate() {
            if target.block.0 as usize == found.header {
                edges.push((index, position));
            }
        }
    }
    edges
}

/// Makes sure a loop entered by a conditional jump has a block of its own
/// in front, where moved instructions can go.
fn add_preheaders(f: &mut Function) {
    for found in find_loops(f) {
        let [(source, position)] = outside_edges(f, &found)[..] else {
            continue;
        };
        if matches!(f.blocks[source].term, Terminator::Jump(_)) {
            continue;
        }

        let new_block = BlockId(f.blocks.len() as u32);
        let mut targets = f.blocks[source].term.targets_mut();
        let edge: Target = std::mem::replace(
            targets[position],
            Target {
                block: new_block,
                args: Vec::new(),
            },
        );
        f.blocks.push(Block {
            params: Vec::new(),
            insts: Vec::new(),
            term: Terminator::Jump(edge),
            term_frees: Vec::new(),
        });
    }
}

fn preheader(f: &Function, found: &Loop) -> Option<usize> {
    let [(source, _)] = outside_edges(f, found)[..] else {
        return None;
    };
    matches!(f.blocks[source].term, Terminator::Jump(_)).then_some(source)
}

fn is_number_or_string(f: &Function, reg: Reg) -> bool {
    // `bytes` can be written in place, like an array.
    matches!(&f.types[reg.0 as usize], Ty::Prim(ty) if *ty != kmpl_ast::Type::Bytes)
}

fn hoist_from(f: &mut Function, found: &Loop, preheader: usize) -> bool {
    let mut defined_inside: HashSet<Reg> = HashSet::new();
    for &b in &found.body {
        defined_inside.extend(f.blocks[b].params.iter().copied());
        defined_inside.extend(f.blocks[b].insts.iter().filter_map(|i| i.dest));
    }

    let mut order: Vec<usize> = found.body.iter().copied().collect();
    order.sort_unstable();

    let mut moved_any = false;
    loop {
        let mut moved = false;
        for &b in &order {
            let mut position = 0;
            while position < f.blocks[b].insts.len() {
                let inst = &f.blocks[b].insts[position];
                let mut invariant = true;
                inst.op
                    .for_each_use(|reg| invariant &= !defined_inside.contains(&reg));

                let movable = invariant
                    && is_pure(&inst.op, f)
                    && inst.dest.is_some_and(|dest| is_number_or_string(f, dest));
                if !movable {
                    position += 1;
                    continue;
                }

                let inst = f.blocks[b].insts.remove(position);
                defined_inside.remove(&inst.dest.expect("checked by `movable`"));
                f.blocks[preheader].insts.push(inst);
                moved = true;
            }
        }
        if !moved {
            return moved_any;
        }
        moved_any = true;
    }
}
