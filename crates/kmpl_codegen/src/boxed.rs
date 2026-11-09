//! Values that stay `Value`s of the runtime: strings, arrays, tuples,
//! structs, `complex` and vectors.
//!
//! Each register of such a type has a stack slot holding an owned pointer,
//! or null. Writing a slot releases what it held, as writing a register of
//! the VM does, so reference counts are the same and an array is copied
//! exactly when the VM would copy it.

use inkwell::intrinsics::Intrinsic;
use inkwell::values::{BasicValue, BasicValueEnum, IntValue, PointerValue};
use kmpl_ast::{Ty, Type, UnOp};
use kmpl_builtins::abi;
use kmpl_ir::{Inst, Op, Reg, Step};

use crate::memory::element;
use crate::translate::{Current, Result, Translator};
use crate::vector::vector_size;

/// The two words of an `abi::Slot`.
pub(crate) type SlotWords<'ctx> = (IntValue<'ctx>, IntValue<'ctx>);

impl<'ctx, 'm> Translator<'ctx, 'm> {
    fn int(&self, value: u64) -> IntValue<'ctx> {
        self.context.i64_type().const_int(value, false)
    }

    pub(crate) fn load_pointer(&self, slot: PointerValue<'ctx>) -> Result<PointerValue<'ctx>> {
        Ok(self
            .builder
            .build_load(self.pointer_type(), slot, "")?
            .into_pointer_value())
    }

    /// Takes the pointer out of `slot`, leaving it empty.
    pub(crate) fn take(&self, slot: PointerValue<'ctx>) -> Result<PointerValue<'ctx>> {
        let pointer = self.load_pointer(slot)?;
        self.builder
            .build_store(slot, self.pointer_type().const_null())?;
        Ok(pointer)
    }

    /// Releases what `slot` holds and leaves it empty.
    pub(crate) fn release(
        &self,
        slot: PointerValue<'ctx>,
        current: &Current<'ctx, 'm>,
    ) -> Result<()> {
        let pointer = self.take(slot)?;
        let is_null = self.builder.build_is_null(pointer, "")?;
        let release = self.context.append_basic_block(current.llvm, "");
        let done = self.context.append_basic_block(current.llvm, "");
        self.builder
            .build_conditional_branch(is_null, done, release)?;
        self.builder.position_at_end(release);
        self.builder
            .build_call(self.runtime.drop, &[pointer.into()], "")?;
        self.builder.build_unconditional_branch(done)?;
        self.builder.position_at_end(done);
        Ok(())
    }

    pub(crate) fn clone_value(&self, pointer: PointerValue<'ctx>) -> Result<PointerValue<'ctx>> {
        Ok(self
            .builder
            .build_call(self.runtime.clone, &[pointer.into()], "")?
            .try_as_basic_value()
            .basic()
            .expect("the runtime returns a pointer")
            .into_pointer_value())
    }

    /// `reg` as an argument. The runtime keeps a number, and only looks at
    /// a boxed value.
    fn slot_of(&self, reg: Reg, current: &Current<'ctx, 'm>) -> Result<SlotWords<'ctx>> {
        let i64_type = self.context.i64_type();
        if let Some(slot) = current.slot(reg) {
            let pointer = self.load_pointer(slot)?;
            let bits = self.builder.build_ptr_to_int(pointer, i64_type, "")?;
            return Ok((self.int(abi::TAG_BORROW), bits));
        }

        if vector_size(current.ty(reg)).is_some() {
            return self.boxed_vector(reg, current);
        }

        let value = current.value(reg);
        match current.type_kind(reg) {
            abi::TAG_INT => Ok((self.int(abi::TAG_INT), value.into_int_value())),
            abi::TAG_REAL => {
                let bits = self.builder.build_bit_cast(value, i64_type, "")?;
                Ok((self.int(abi::TAG_REAL), bits.into_int_value()))
            }
            _ => {
                let bits = self
                    .builder
                    .build_int_z_extend(value.into_int_value(), i64_type, "")?;
                Ok((self.int(abi::TAG_BOOL), bits))
            }
        }
    }

    /// A vector register packed into a new `Value` for the runtime.
    fn boxed_vector(&self, reg: Reg, current: &Current<'ctx, 'm>) -> Result<SlotWords<'ctx>> {
        let kind = match current.ty(reg) {
            Ty::Prim(Type::Complex) => abi::KIND_COMPLEX,
            Ty::Prim(Type::Vec2) => abi::KIND_VEC2,
            Ty::Prim(Type::Vec3) => abi::KIND_VEC3,
            _ => abi::KIND_VEC4,
        };
        let parts = self.components(current.value(reg).into_vector_value())?;
        let unused = self.context.f64_type().const_float(0.0);
        let part = |position: usize| parts.get(position).copied().unwrap_or(unused).into();
        let pointer = self
            .builder
            .build_call(
                self.runtime.box_vector,
                &[self.int(kind).into(), part(0), part(1), part(2), part(3)],
                "",
            )?
            .try_as_basic_value()
            .basic()
            .expect("the runtime returns a pointer")
            .into_pointer_value();
        let bits = self
            .builder
            .build_ptr_to_int(pointer, self.context.i64_type(), "")?;
        Ok((self.int(abi::TAG_MOVE), bits))
    }

    /// `reg` as an argument the runtime will own.
    fn moved_slot(&self, reg: Reg, current: &Current<'ctx, 'm>) -> Result<SlotWords<'ctx>> {
        let slot = current.slot(reg).expect("only boxed registers are moved");
        let pointer = self.take(slot)?;
        let bits = self
            .builder
            .build_ptr_to_int(pointer, self.context.i64_type(), "")?;
        Ok((self.int(abi::TAG_MOVE), bits))
    }

    /// A constant text as a slot: its address and its length.
    fn text_slot(&self, text: &str) -> Result<SlotWords<'ctx>> {
        let (pointer, length) = self.string(text)?;
        let address = self
            .builder
            .build_ptr_to_int(pointer, self.context.i64_type(), "")?;
        Ok((address, length))
    }

    fn write_arguments(
        &self,
        slots: &[SlotWords<'ctx>],
        current: &Current<'ctx, 'm>,
    ) -> Result<()> {
        let i64_type = self.context.i64_type();
        for (position, (tag, bits)) in slots.iter().enumerate() {
            let index = 2 * position as u64;
            let tag_address = element(&self.builder, i64_type, current.arguments, index)?;
            self.builder.build_store(tag_address, *tag)?;
            let bits_address = element(&self.builder, i64_type, current.arguments, index + 1)?;
            self.builder.build_store(bits_address, *bits)?;
        }
        Ok(())
    }

    /// What the runtime wrote as the result, in the form of `dest`.
    fn read_result(&self, dest: Reg, current: &Current<'ctx, 'm>) -> Result<BasicValueEnum<'ctx>> {
        let i64_type = self.context.i64_type();
        let address = element(&self.builder, i64_type, current.result, 1)?;
        let bits = self
            .builder
            .build_load(i64_type, address, "")?
            .into_int_value();
        if current.slot(dest).is_some() {
            return Ok(self
                .builder
                .build_int_to_ptr(bits, self.pointer_type(), "")?
                .into());
        }
        if let Some(size) = vector_size(current.ty(dest)) {
            let boxed = self
                .builder
                .build_int_to_ptr(bits, self.pointer_type(), "")?;
            self.builder.build_call(
                self.runtime.unbox_vector,
                &[boxed.into(), current.result.into()],
                "",
            )?;
            let vector = self
                .builder
                .build_load(self.vector_type(size), current.result, "")?;
            // The buffer holds 8-byte words; a vector would expect more.
            vector
                .as_instruction_value()
                .expect("a load is an instruction")
                .set_alignment(8)
                .expect("8 is a power of two");
            return Ok(vector);
        }
        Ok(match current.type_kind(dest) {
            abi::TAG_INT => bits.into(),
            abi::TAG_REAL => self
                .builder
                .build_bit_cast(bits, self.context.f64_type(), "")?,
            _ => self
                .builder
                .build_int_truncate(bits, self.context.bool_type(), "")?
                .into(),
        })
    }

    fn runtime_op(
        &self,
        code: u64,
        extra: u64,
        slots: &[SlotWords<'ctx>],
        inst: &Inst,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        self.write_arguments(slots, current)?;
        let (location, location_length) = self.location(inst.span)?;
        self.builder.build_call(
            self.runtime.op,
            &[
                self.int(code).into(),
                self.int(extra).into(),
                current.arguments.into(),
                self.int(slots.len() as u64).into(),
                current.result.into(),
                location.into(),
                location_length.into(),
            ],
            "",
        )?;
        let dest = inst.dest.expect("these operations have a result");
        Ok(Some(self.read_result(dest, current)?))
    }

    fn slots_of(&self, regs: &[Reg], current: &Current<'ctx, 'm>) -> Result<Vec<SlotWords<'ctx>>> {
        regs.iter().map(|r| self.slot_of(*r, current)).collect()
    }

    /// A path as slots: a field, or an index followed by where a bad index
    /// is reported.
    fn path_slots(
        &self,
        path: &[Step],
        slots: &mut Vec<SlotWords<'ctx>>,
        current: &Current<'ctx, 'm>,
    ) -> Result<()> {
        for step in path {
            match step {
                Step::Field(position) => {
                    slots.push((self.int(abi::TAG_FIELD), self.int(*position as u64)));
                }
                Step::Index(reg, span) => {
                    slots.push(self.slot_of(*reg, current)?);
                    slots.push(self.text_slot(&(self.locate)(*span))?);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn boxed_string(
        &self,
        text: &str,
        inst: &Inst,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        let slots = [self.text_slot(text)?];
        self.runtime_op(abi::OP_STRING, 0, &slots, inst, current)
    }

    pub(crate) fn boxed_unary(
        &self,
        op: UnOp,
        a: Reg,
        inst: &Inst,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        let slots = [self.slot_of(a, current)?];
        self.runtime_op(abi::OP_UNARY, abi::un_op_code(op), &slots, inst, current)
    }

    pub(crate) fn boxed_binary(
        &self,
        code: u64,
        a: Reg,
        b: Reg,
        inst: &Inst,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        let slots = [self.slot_of(a, current)?, self.slot_of(b, current)?];
        self.runtime_op(abi::OP_BINARY, code, &slots, inst, current)
    }

    /// The operations that only exist on boxed values.
    pub(crate) fn boxed_operation(
        &self,
        inst: &Inst,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        let run = |code, extra, slots: &[SlotWords<'ctx>]| {
            self.runtime_op(code, extra, slots, inst, current)
        };
        match &inst.op {
            Op::MakeArray(items) => run(abi::OP_MAKE_ARRAY, 0, &self.slots_of(items, current)?),
            Op::ArrayFill { value, len } => run(
                abi::OP_ARRAY_FILL,
                0,
                &self.slots_of(&[*value, *len], current)?,
            ),
            Op::ArrayLen(array) => run(abi::OP_ARRAY_LEN, 0, &self.slots_of(&[*array], current)?),
            Op::Index { base, index } => {
                run(abi::OP_INDEX, 0, &self.slots_of(&[*base, *index], current)?)
            }
            Op::Slice {
                base,
                start,
                end,
                inclusive,
            } => {
                let mut flags = 0;
                if *inclusive {
                    flags |= abi::SLICE_INCLUSIVE;
                }
                if start.is_some() {
                    flags |= abi::SLICE_HAS_START;
                }
                if end.is_some() {
                    flags |= abi::SLICE_HAS_END;
                }
                let mut regs = vec![*base];
                regs.extend(start.iter().chain(end.iter()).copied());
                run(abi::OP_SLICE, flags, &self.slots_of(&regs, current)?)
            }
            Op::MakeTuple(items) => run(abi::OP_MAKE_TUPLE, 0, &self.slots_of(items, current)?),
            Op::TupleGet { tuple, index } => run(
                abi::OP_TUPLE_GET,
                *index as u64,
                &self.slots_of(&[*tuple], current)?,
            ),
            Op::MakeStruct { id, values } => run(
                abi::OP_MAKE_STRUCT,
                *id as u64,
                &self.slots_of(values, current)?,
            ),
            Op::FieldGet { base, index } => run(
                abi::OP_FIELD_GET,
                *index as u64,
                &self.slots_of(&[*base], current)?,
            ),
            Op::PathGet { base, path } => {
                let mut slots = vec![self.slot_of(*base, current)?];
                self.path_slots(path, &mut slots, current)?;
                run(abi::OP_PATH_GET, 0, &slots)
            }
            Op::PathSet {
                base,
                path,
                value,
                take,
            } => {
                let root = match take {
                    true => self.moved_slot(*base, current)?,
                    false => self.slot_of(*base, current)?,
                };
                let mut slots = vec![root, self.slot_of(*value, current)?];
                self.path_slots(path, &mut slots, current)?;
                run(abi::OP_PATH_SET, 0, &slots)
            }
            other => unreachable!("{other:?} is translated elsewhere"),
        }
    }

    /// The built-ins that are a single machine operation with exactly the
    /// result of their Rust version.
    fn direct_builtin(
        &self,
        id: usize,
        args: &[Reg],
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        let name = self.ir.builtins[id].as_str();
        if let Some(value) = self.vector_builtin(name, args, current)? {
            return Ok(Some(value));
        }
        let [arg] = args else {
            return Ok(None);
        };
        let f64_type = self.context.f64_type();
        match (name, current.ty(*arg)) {
            ("real", Ty::Prim(Type::Int)) => Ok(Some(
                self.builder
                    .build_signed_int_to_float(current.int(*arg), f64_type, "")?
                    .into(),
            )),
            ("real", Ty::Prim(Type::Real)) => Ok(Some(current.value(*arg))),
            ("sqrt" | "abs" | "floor" | "ceil" | "round", Ty::Prim(Type::Real)) => {
                let intrinsic = match name {
                    "abs" => "llvm.fabs".to_owned(),
                    other => format!("llvm.{other}"),
                };
                let declaration = Intrinsic::find(&intrinsic)
                    .expect("LLVM has this intrinsic")
                    .get_declaration(&self.llvm, &[f64_type.into()])
                    .expect("the intrinsic takes this type");
                let call =
                    self.builder
                        .build_call(declaration, &[current.value(*arg).into()], "")?;
                Ok(call.try_as_basic_value().basic())
            }
            _ => Ok(None),
        }
    }

    /// Calls entry `id` of `Module::builtins`. An argument used for the
    /// last time is moved, so the built-in can reuse it.
    pub(crate) fn builtin(
        &self,
        id: usize,
        args: &[Reg],
        inst: &Inst,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        if let Some(value) = self.direct_builtin(id, args, current)? {
            return Ok(Some(value));
        }

        let mut slots = Vec::with_capacity(args.len());
        for (position, reg) in args.iter().enumerate() {
            let last_use = inst.frees.contains(reg) && !args[position + 1..].contains(reg);
            slots.push(if last_use && current.slot(*reg).is_some() {
                self.moved_slot(*reg, current)?
            } else {
                self.slot_of(*reg, current)?
            });
        }
        self.write_arguments(&slots, current)?;

        // A null result tells the runtime that nobody wants the value.
        let result = match inst.dest {
            Some(_) => current.result,
            None => self.pointer_type().const_null(),
        };
        let (location, location_length) = self.location(inst.span)?;
        self.builder.build_call(
            self.runtime.builtin,
            &[
                self.int(self.builtins[id]).into(),
                current.arguments.into(),
                self.int(args.len() as u64).into(),
                result.into(),
                location.into(),
                location_length.into(),
            ],
            "",
        )?;
        inst.dest
            .map(|dest| self.read_result(dest, current))
            .transpose()
    }
}
