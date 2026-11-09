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
use crate::{Options, timed};

pub fn object_code(
    module: &Module,
    locate: &dyn Fn(Span) -> String,
    options: Options,
) -> Result<Vec<u8>, String> {
    let context = Context::create();
    let machine = target_machine()?;
    let llvm = optimized(&context, &machine, module, locate, options)?;
    let object = timed(options, "llvm: machine code", || {
        machine.write_to_memory_buffer(&llvm, FileType::Object)
    })
    .map_err(|e| e.to_string())?;
    Ok(object.as_slice().to_vec())
}

pub fn text(
    module: &Module,
    locate: &dyn Fn(Span) -> String,
    options: Options,
) -> Result<String, String> {
    let context = Context::create();
    let machine = target_machine()?;
    let llvm = optimized(&context, &machine, module, locate, options)?;
    Ok(llvm.to_string())
}

fn optimized<'ctx>(
    context: &'ctx Context,
    machine: &TargetMachine,
    module: &Module,
    locate: &dyn Fn(Span) -> String,
    options: Options,
) -> Result<inkwell::module::Module<'ctx>, String> {
    let llvm = timed(options, "llvm: translate", || {
        Translator::new(context, module, locate).translate()
    })
    .map_err(|e| e.to_string())?;

    llvm.set_triple(&machine.get_triple());
    llvm.set_data_layout(&machine.get_target_data().get_data_layout());
    record_compiler(context, &llvm)?;
    llvm.verify().map_err(|e| {
        format!(
            "kmpl: generated invalid code, this is a bug in kmpl: {}",
            e.to_string()
        )
    })?;

    let pass_options = PassBuilderOptions::create();
    pass_options.set_debug_logging(options.verbosity >= 2);
    // No fast-math flags: `real` arithmetic must not be rearranged.
    timed(options, "llvm: optimize", || {
        llvm.run_passes("default<O2>", machine, pass_options)
    })
    .map_err(|e| e.to_string())?;
    Ok(llvm)
}

/// Puts `kmpl <version>` into the `.comment` section of the executable, next
/// to what the C and Rust compilers leave there.
fn record_compiler<'ctx>(
    context: &'ctx Context,
    llvm: &inkwell::module::Module<'ctx>,
) -> Result<(), String> {
    let name = context.metadata_string(concat!("kmpl ", env!("CARGO_PKG_VERSION")));
    let node = context.metadata_node(&[name.into()]);
    llvm.add_global_metadata("llvm.ident", &node)
        .map_err(|e| e.to_string())
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
