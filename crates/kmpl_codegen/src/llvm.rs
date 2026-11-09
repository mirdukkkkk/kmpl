//! IR to LLVM to object code.

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use kmpl_ir::{Module, Terminator};

/// The symbol `kmpl_rt` calls to start the program.
const ENTRY: &str = "kmpl_program";

pub fn object_code(module: &Module) -> Result<Vec<u8>, String> {
    for function in [module.init, module.main] {
        let function = &module.functions[function.0 as usize];
        let empty = matches!(&function.blocks[..], [block] if block.insts.is_empty()
            && block.term == Terminator::Return(None));
        if !empty {
            return Err("kmpl build: this program is not supported yet".to_owned());
        }
    }

    let context = Context::create();
    let llvm = context.create_module("kmpl");
    let builder = context.create_builder();
    let entry = llvm.add_function(ENTRY, context.void_type().fn_type(&[], false), None);
    builder.position_at_end(context.append_basic_block(entry, "start"));
    builder.build_return(None).map_err(|e| e.to_string())?;

    let machine = target_machine()?;
    llvm.set_triple(&machine.get_triple());
    llvm.set_data_layout(&machine.get_target_data().get_data_layout());
    let object = machine
        .write_to_memory_buffer(&llvm, FileType::Object)
        .map_err(|e| e.to_string())?;
    Ok(object.as_slice().to_vec())
}

/// Code for a generic CPU of this architecture, so a built program also runs
/// on other machines. Position independent, as linkers expect by default.
fn target_machine() -> Result<TargetMachine, String> {
    Target::initialize_native(&InitializationConfig::default())?;
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| format!("kmpl build: cannot generate code for {triple}"))
}
