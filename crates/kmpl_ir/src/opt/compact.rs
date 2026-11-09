//! Renumbers registers without gaps. Passes leave unused numbers behind,
//! and every call reserves space for all of a function's registers.

use std::collections::HashMap;

use crate::{Function, Reg};

pub fn compact(f: &mut Function) {
    let mut new_numbers: HashMap<Reg, Reg> = HashMap::new();
    let mut types = Vec::new();
    let mut assign = |reg: &mut Reg, old_types: &[kmpl_ast::Ty]| {
        let number = Reg(types.len() as u32);
        let new = *new_numbers.entry(*reg).or_insert_with(|| {
            types.push(old_types[reg.0 as usize].clone());
            number
        });
        *reg = new;
    };

    let old_types = std::mem::take(&mut f.types);
    for block in &mut f.blocks {
        for param in &mut block.params {
            assign(param, &old_types);
        }
        for inst in &mut block.insts {
            if let Some(dest) = &mut inst.dest {
                assign(dest, &old_types);
            }
        }
    }
    f.params.clone_from(&f.blocks[0].params);
    f.types = types;

    let renumber = |reg: &mut Reg| *reg = new_numbers[&*reg];
    for block in &mut f.blocks {
        for inst in &mut block.insts {
            inst.op.for_each_use_mut(renumber);
        }
        block.term.for_each_use_mut(renumber);
    }
}
