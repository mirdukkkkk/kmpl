//! `[real]` and `[int]`: their numbers are read and written directly by the
//! generated code.
//!
//! Next to the stack slot of such a register are three more: where its
//! numbers are, how many there are, and whether the array is known not to
//! be shared. LLVM turns all of them into machine registers, so a loop over
//! an array makes no calls to the runtime.

use inkwell::IntPredicate;
use inkwell::types::{BasicTypeEnum, StructType};
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use kmpl_ast::{Span, Ty, Type};
use kmpl_builtins::abi;
use kmpl_ir::{Inst, Op, Reg, Step};

use crate::memory::element_at;
use crate::translate::{Current, Result, Translator};

#[derive(Clone, Copy)]
pub(crate) struct FlatSlots<'ctx> {
    data: PointerValue<'ctx>,
    len: PointerValue<'ctx>,
    unique: PointerValue<'ctx>,
}

/// A copy of what those slots hold, for handing an array to a jump's target.
#[derive(Clone, Copy)]
pub(crate) struct View<'ctx> {
    data: PointerValue<'ctx>,
    len: IntValue<'ctx>,
    unique: IntValue<'ctx>,
}

/// Which form an array of this type has at run time.
pub(crate) fn array_form(ty: &Ty) -> u64 {
    match ty {
        Ty::Array(element) => match **element {
            Ty::Prim(Type::Real) => abi::ARRAY_OF_REALS,
            Ty::Prim(Type::Int) => abi::ARRAY_OF_INTS,
            _ => abi::ARRAY_OF_VALUES,
        },
        _ => abi::ARRAY_OF_VALUES,
    }
}

pub(crate) fn is_flat(ty: &Ty) -> bool {
    array_form(ty) != abi::ARRAY_OF_VALUES
}

impl<'ctx, 'm> Translator<'ctx, 'm> {
    /// The two-word `abi::ArrayView`.
    pub(crate) fn view_type(context: &'ctx inkwell::context::Context) -> StructType<'ctx> {
        let pointer = context.ptr_type(inkwell::AddressSpace::default());
        context.struct_type(&[pointer.into(), context.i64_type().into()], false)
    }

    /// Stack slots for one register. The builder is in the first block.
    pub(crate) fn flat_slots(&self) -> Result<FlatSlots<'ctx>> {
        let builder = &self.builder;
        let data = builder.build_alloca(self.pointer_type(), "")?;
        builder.build_store(data, self.pointer_type().const_null())?;
        let len = builder.build_alloca(self.context.i64_type(), "")?;
        builder.build_store(len, self.context.i64_type().const_zero())?;
        let unique = builder.build_alloca(self.context.bool_type(), "")?;
        builder.build_store(unique, self.context.bool_type().const_zero())?;
        Ok(FlatSlots { data, len, unique })
    }

    fn store_view(&self, slots: FlatSlots<'ctx>, result: BasicValueEnum<'ctx>) -> Result<()> {
        let view = result.into_struct_value();
        let data = self.builder.build_extract_value(view, 0, "")?;
        let len = self.builder.build_extract_value(view, 1, "")?;
        self.builder.build_store(slots.data, data)?;
        self.builder.build_store(slots.len, len)?;
        Ok(())
    }

    /// After a new array was stored in the slot of `reg`. It may be shared.
    pub(crate) fn refresh_view(&self, reg: Reg, current: &Current<'ctx, 'm>) -> Result<()> {
        let Some(slots) = current.flat(reg) else {
            return Ok(());
        };
        let slot = current.slot(reg).expect("a flat array is boxed");
        let pointer = self.load_pointer(slot)?;
        let view = self
            .builder
            .build_call(self.runtime.array_view, &[pointer.into()], "")?
            .try_as_basic_value()
            .basic()
            .expect("the runtime returns a view");
        self.store_view(slots, view)?;
        self.builder
            .build_store(slots.unique, self.context.bool_type().const_zero())?;
        Ok(())
    }

    /// The array of `reg` is about to be shared with something else.
    pub(crate) fn forget_unique(&self, reg: Reg, current: &Current<'ctx, 'm>) -> Result<()> {
        if let Some(slots) = current.flat(reg) {
            self.builder
                .build_store(slots.unique, self.context.bool_type().const_zero())?;
        }
        Ok(())
    }

    /// What a jump's target should know about the array in `arg`. If the
    /// jump clones the array rather than moves it, both ends are now shared.
    pub(crate) fn capture_view(
        &self,
        arg: Reg,
        moved: bool,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<View<'ctx>>> {
        let Some(slots) = current.flat(arg) else {
            return Ok(None);
        };
        let builder = &self.builder;
        let data = builder
            .build_load(self.pointer_type(), slots.data, "")?
            .into_pointer_value();
        let len = builder
            .build_load(self.context.i64_type(), slots.len, "")?
            .into_int_value();
        let unique = match moved {
            true => builder
                .build_load(self.context.bool_type(), slots.unique, "")?
                .into_int_value(),
            false => {
                self.forget_unique(arg, current)?;
                self.context.bool_type().const_zero()
            }
        };
        Ok(Some(View { data, len, unique }))
    }

    pub(crate) fn restore_view(
        &self,
        param: Reg,
        view: View<'ctx>,
        current: &Current<'ctx, 'm>,
    ) -> Result<()> {
        let slots = current.flat(param).expect("the argument is flat too");
        self.builder.build_store(slots.data, view.data)?;
        self.builder.build_store(slots.len, view.len)?;
        self.builder.build_store(slots.unique, view.unique)?;
        Ok(())
    }

    fn element_type(&self, array: Reg, current: &Current<'ctx, 'm>) -> BasicTypeEnum<'ctx> {
        match array_form(current.ty(array)) {
            abi::ARRAY_OF_REALS => self.context.f64_type().into(),
            _ => self.context.i64_type().into(),
        }
    }

    fn check_index(
        &self,
        index: IntValue<'ctx>,
        len: IntValue<'ctx>,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<()> {
        // Unsigned: a negative index looks huge.
        let inside = self
            .builder
            .build_int_compare(IntPredicate::ULT, index, len, "")?;
        let bad = self.context.append_basic_block(current.llvm, "");
        let fine = self.context.append_basic_block(current.llvm, "");
        self.builder.build_conditional_branch(inside, fine, bad)?;

        self.builder.position_at_end(bad);
        let (location, location_length) = self.location(span)?;
        self.builder.build_call(
            self.runtime.index_error,
            &[
                index.into(),
                len.into(),
                location.into(),
                location_length.into(),
            ],
            "",
        )?;
        self.builder.build_unreachable()?;

        self.builder.position_at_end(fine);
        Ok(())
    }

    fn load_len(&self, slots: FlatSlots<'ctx>) -> Result<IntValue<'ctx>> {
        Ok(self
            .builder
            .build_load(self.context.i64_type(), slots.len, "")?
            .into_int_value())
    }

    fn load_data(&self, slots: FlatSlots<'ctx>) -> Result<PointerValue<'ctx>> {
        Ok(self
            .builder
            .build_load(self.pointer_type(), slots.data, "")?
            .into_pointer_value())
    }

    fn read(
        &self,
        array: Reg,
        index: Reg,
        span: Span,
        slots: FlatSlots<'ctx>,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let position = current.int(index);
        self.check_index(position, self.load_len(slots)?, span, current)?;
        let ty = self.element_type(array, current);
        let address = element_at(&self.builder, ty, self.load_data(slots)?, position)?;
        self.builder.build_load(ty, address, "")
    }

    /// `a[i] = v` when `a` is not used afterwards: the array moves to the
    /// destination register and is changed in place if nothing shares it.
    fn write(
        &self,
        array: Reg,
        index: Reg,
        value: Reg,
        span: Span,
        dest: Reg,
        current: &Current<'ctx, 'm>,
    ) -> Result<()> {
        let builder = &self.builder;
        let slots = current.flat(array).expect("the caller checked it is flat");
        let pointer = self.take(current.slot(array).expect("a flat array is boxed"))?;

        let unique = builder
            .build_load(self.context.bool_type(), slots.unique, "")?
            .into_int_value();
        let make_unique = self.context.append_basic_block(current.llvm, "");
        let ready = self.context.append_basic_block(current.llvm, "");
        builder.build_conditional_branch(unique, ready, make_unique)?;
        builder.position_at_end(make_unique);
        let view = builder
            .build_call(self.runtime.array_unique, &[pointer.into()], "")?
            .try_as_basic_value()
            .basic()
            .expect("the runtime returns a view");
        self.store_view(slots, view)?;
        builder.build_unconditional_branch(ready)?;
        builder.position_at_end(ready);

        let position = current.int(index);
        let len = self.load_len(slots)?;
        self.check_index(position, len, span, current)?;
        let ty = self.element_type(array, current);
        let data = self.load_data(slots)?;
        let address = element_at(builder, ty, data, position)?;
        builder.build_store(address, current.value(value))?;

        let dest_slot = current.slot(dest).expect("the result is boxed too");
        self.release(dest_slot, current)?;
        builder.build_store(dest_slot, pointer)?;
        let unique = self.context.bool_type().const_all_ones();
        self.restore_view(dest, View { data, len, unique }, current)
    }

    /// The operations on a flat array done without calling the runtime.
    /// `None` if this instruction is not one of them.
    #[allow(clippy::type_complexity)]
    pub(crate) fn flat_operation(
        &self,
        inst: &Inst,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<Option<BasicValueEnum<'ctx>>>> {
        let result = match &inst.op {
            Op::ArrayLen(array) => match current.flat(*array) {
                Some(slots) => Some(self.load_len(slots)?.into()),
                None => return Ok(None),
            },
            Op::Index { base, index } => match current.flat(*base) {
                Some(slots) => Some(self.read(*base, *index, inst.span, slots, current)?),
                None => return Ok(None),
            },
            Op::PathGet { base, path } => match (current.flat(*base), path.as_slice()) {
                (Some(slots), [Step::Index(index, span)]) => {
                    Some(self.read(*base, *index, *span, slots, current)?)
                }
                _ => return Ok(None),
            },
            Op::PathSet {
                base,
                path,
                value,
                take: true,
            } => match (current.flat(*base), path.as_slice()) {
                (Some(_), [Step::Index(index, span)]) => {
                    let dest = inst.dest.expect("writing makes a new register");
                    self.write(*base, *index, *value, *span, dest, current)?;
                    None
                }
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}
