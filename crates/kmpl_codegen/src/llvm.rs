//! IR to LLVM to object code.

use std::sync::OnceLock;

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use kmpl_ast::Span;
use kmpl_ir::Module;

use crate::translate::Translator;

pub fn object_code(module: &Module, locate: &dyn Fn(Span) -> String) -> Result<Vec<u8>, String> {
    let context = Context::create();
    let llvm = Translator::new(&context, module, locate)
        .translate()
        .map_err(|e| e.to_string())?;

    let machine = target_machine()?;
    llvm.set_triple(&machine.get_triple());
    llvm.set_data_layout(&machine.get_target_data().get_data_layout());
    llvm.verify().map_err(|e| {
        format!(
            "kmpl: generated invalid code, this is a bug in kmpl: {}",
            e.to_string()
        )
    })?;

    // No fast-math flags: `real` arithmetic must not be rearranged.
    llvm.run_passes("default<O2>", &machine, PassBuilderOptions::create())
        .map_err(|e| e.to_string())?;

    let object = machine
        .write_to_memory_buffer(&llvm, FileType::Object)
        .map_err(|e| e.to_string())?;
    Ok(object.as_slice().to_vec())
}

/// Code for a generic CPU of this architecture, so a built program also runs
/// on other machines. Position independent, as linkers expect by default.
fn target_machine() -> Result<TargetMachine, String> {
    // Once per process: builds may run at the same time in tests.
    static INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();
    INITIALIZED
        .get_or_init(|| Target::initialize_native(&InitializationConfig::default()))
        .clone()?;

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
        .ok_or_else(|| format!("kmpl: cannot generate code for {triple}"))
}
