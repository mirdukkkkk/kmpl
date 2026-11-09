//! Arithmetic and comparisons on `int`, `real` and `bool`, which live in
//! machine registers. Failures say what `ops.rs` says.

use inkwell::intrinsics::Intrinsic;
use inkwell::values::{BasicValueEnum, IntValue};
use inkwell::{FloatPredicate, IntPredicate};
use kmpl_ast::{BinOp, Span, Ty, Type};
use kmpl_builtins::ops;
use kmpl_ir::Reg;

use crate::translate::{Current, Result, Translator};

impl<'ctx, 'm> Translator<'ctx, 'm> {
    pub(crate) fn number_neg(
        &self,
        a: Reg,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        Ok(match current.ty(a) {
            Ty::Prim(Type::Real) => self
                .builder
                .build_float_neg(current.value(a).into_float_value(), "")?
                .into(),
            _ => {
                let zero = self.context.i64_type().const_zero();
                self.checked(
                    "llvm.ssub.with.overflow",
                    zero,
                    current.int(a),
                    span,
                    current,
                )?
                .into()
            }
        })
    }

    pub(crate) fn number_binary(
        &self,
        op: BinOp,
        a: Reg,
        b: Reg,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        match current.ty(a) {
            Ty::Prim(Type::Real) => self.real_binary(op, a, b, current),
            _ => self.int_binary(op, current.int(a), current.int(b), span, current),
        }
    }

    fn real_binary(
        &self,
        op: BinOp,
        a: Reg,
        b: Reg,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let builder = &self.builder;
        let (x, y) = (
            current.value(a).into_float_value(),
            current.value(b).into_float_value(),
        );
        let compare = |predicate| -> Result<BasicValueEnum<'ctx>> {
            Ok(builder.build_float_compare(predicate, x, y, "")?.into())
        };
        match op {
            BinOp::Add => Ok(builder.build_float_add(x, y, "")?.into()),
            BinOp::Sub => Ok(builder.build_float_sub(x, y, "")?.into()),
            BinOp::Mul => Ok(builder.build_float_mul(x, y, "")?.into()),
            BinOp::Div => Ok(builder.build_float_div(x, y, "")?.into()),
            BinOp::Rem => Ok(builder.build_float_rem(x, y, "")?.into()),
            BinOp::Eq => compare(FloatPredicate::OEQ),
            // Unordered: `NaN != NaN` is true.
            BinOp::Ne => compare(FloatPredicate::UNE),
            BinOp::Lt => compare(FloatPredicate::OLT),
            BinOp::Le => compare(FloatPredicate::OLE),
            BinOp::Gt => compare(FloatPredicate::OGT),
            BinOp::Ge => compare(FloatPredicate::OGE),
            BinOp::And | BinOp::Or => unreachable!("lowered to branches"),
        }
    }

    /// Also compares `bool`s, which are integers here.
    fn int_binary(
        &self,
        op: BinOp,
        x: IntValue<'ctx>,
        y: IntValue<'ctx>,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let builder = &self.builder;
        let compare = |predicate| -> Result<BasicValueEnum<'ctx>> {
            Ok(builder.build_int_compare(predicate, x, y, "")?.into())
        };
        match op {
            BinOp::Add => Ok(self
                .checked("llvm.sadd.with.overflow", x, y, span, current)?
                .into()),
            BinOp::Sub => Ok(self
                .checked("llvm.ssub.with.overflow", x, y, span, current)?
                .into()),
            BinOp::Mul => Ok(self
                .checked("llvm.smul.with.overflow", x, y, span, current)?
                .into()),
            BinOp::Div => {
                self.check_divisor(x, y, span, current)?;
                Ok(builder.build_int_signed_div(x, y, "")?.into())
            }
            BinOp::Rem => {
                self.check_divisor(x, y, span, current)?;
                Ok(builder.build_int_signed_rem(x, y, "")?.into())
            }
            BinOp::Eq => compare(IntPredicate::EQ),
            BinOp::Ne => compare(IntPredicate::NE),
            BinOp::Lt => compare(IntPredicate::SLT),
            BinOp::Le => compare(IntPredicate::SLE),
            BinOp::Gt => compare(IntPredicate::SGT),
            BinOp::Ge => compare(IntPredicate::SGE),
            BinOp::And | BinOp::Or => unreachable!("lowered to branches"),
        }
    }

    pub(crate) fn with_overflow(
        &self,
        name: &str,
        x: IntValue<'ctx>,
        y: IntValue<'ctx>,
    ) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let declaration = Intrinsic::find(name)
            .expect("LLVM has this intrinsic")
            .get_declaration(&self.llvm, &[self.context.i64_type().into()])
            .expect("the intrinsic takes this type");
        let pair = self
            .builder
            .build_call(declaration, &[x.into(), y.into()], "")?
            .try_as_basic_value()
            .basic()
            .expect("the intrinsic returns a pair")
            .into_struct_value();
        let value = self.builder.build_extract_value(pair, 0, "")?;
        let overflow = self.builder.build_extract_value(pair, 1, "")?;
        Ok((value.into_int_value(), overflow.into_int_value()))
    }

    fn checked(
        &self,
        name: &str,
        x: IntValue<'ctx>,
        y: IntValue<'ctx>,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<IntValue<'ctx>> {
        let (value, overflow) = self.with_overflow(name, x, y)?;
        self.fail_if(overflow, ops::OVERFLOW, span, current)?;
        Ok(value)
    }

    /// Stops the program if `x / y` or `x % y` is not defined.
    fn check_divisor(
        &self,
        x: IntValue<'ctx>,
        y: IntValue<'ctx>,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<()> {
        let builder = &self.builder;
        let i64_type = self.context.i64_type();
        let by_zero = builder.build_int_compare(IntPredicate::EQ, y, i64_type.const_zero(), "")?;
        self.fail_if(by_zero, ops::DIVISION_BY_ZERO, span, current)?;

        // The one division that does not fit: LLVM leaves it undefined.
        let minimum = i64_type.const_int(i64::MIN as u64, true);
        let minus_one = i64_type.const_int(-1i64 as u64, true);
        let is_minimum = builder.build_int_compare(IntPredicate::EQ, x, minimum, "")?;
        let is_minus_one = builder.build_int_compare(IntPredicate::EQ, y, minus_one, "")?;
        let overflow = builder.build_and(is_minimum, is_minus_one, "")?;
        self.fail_if(overflow, ops::OVERFLOW, span, current)
    }
}
