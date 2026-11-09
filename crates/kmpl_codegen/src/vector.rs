//! `complex` and `vec2/3/4` live in LLVM vectors, in machine registers.
//! The arithmetic is written out per component, exactly as `ops.rs` does it;
//! LLVM turns it back into vector instructions where that is the same.

use inkwell::FloatPredicate;
use inkwell::types::VectorType;
use inkwell::values::{BasicValueEnum, FloatValue, IntValue, VectorValue};
use kmpl_ast::{BinOp, Ty, Type, UnOp};
use kmpl_ir::Reg;

use crate::translate::{Current, Result, Translator};

/// How many components a value of this type has in a vector register, or
/// `None` if it does not live in one.
pub(crate) fn vector_size(ty: &Ty) -> Option<u32> {
    match ty {
        Ty::Prim(Type::Complex | Type::Vec2) => Some(2),
        Ty::Prim(Type::Vec3) => Some(3),
        Ty::Prim(Type::Vec4) => Some(4),
        _ => None,
    }
}

impl<'ctx, 'm> Translator<'ctx, 'm> {
    pub(crate) fn vector_type(&self, size: u32) -> VectorType<'ctx> {
        self.context.f64_type().vec_type(size)
    }

    fn index(&self, position: u64) -> IntValue<'ctx> {
        self.context.i32_type().const_int(position, false)
    }

    pub(crate) fn vector_of(&self, components: &[FloatValue<'ctx>]) -> Result<VectorValue<'ctx>> {
        let mut vector = self.vector_type(components.len() as u32).get_undef();
        for (position, component) in components.iter().enumerate() {
            vector = self.builder.build_insert_element(
                vector,
                *component,
                self.index(position as u64),
                "",
            )?;
        }
        Ok(vector)
    }

    pub(crate) fn component(
        &self,
        vector: VectorValue<'ctx>,
        position: u64,
    ) -> Result<FloatValue<'ctx>> {
        Ok(self
            .builder
            .build_extract_element(vector, self.index(position), "")?
            .into_float_value())
    }

    /// All components of a vector, as an array to work with.
    pub(crate) fn components(&self, vector: VectorValue<'ctx>) -> Result<Vec<FloatValue<'ctx>>> {
        (0..u64::from(vector.get_type().get_size()))
            .map(|position| self.component(vector, position))
            .collect()
    }

    pub(crate) fn vector_constant(&self, components: &[f64]) -> Result<BasicValueEnum<'ctx>> {
        let f64_type = self.context.f64_type();
        let values: Vec<FloatValue> = components
            .iter()
            .map(|c| f64_type.const_float(*c))
            .collect();
        Ok(self.vector_of(&values)?.into())
    }

    fn map(
        &self,
        components: &[FloatValue<'ctx>],
        f: impl Fn(FloatValue<'ctx>) -> Result<FloatValue<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let mapped = components
            .iter()
            .map(|c| f(*c))
            .collect::<Result<Vec<_>>>()?;
        Ok(self.vector_of(&mapped)?.into())
    }

    pub(crate) fn vector_unary(
        &self,
        op: UnOp,
        a: Reg,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        debug_assert!(matches!(op, UnOp::Neg));
        let vector = current.value(a).into_vector_value();
        let builder = &self.builder;
        let parts = self.components(vector)?;
        match current.ty(a) {
            // `0 - z`, not `-z`, see `ops::unary`.
            Ty::Prim(Type::Complex) => {
                let zero = self.context.f64_type().const_float(0.0);
                self.map(&parts, |c| builder.build_float_sub(zero, c, ""))
            }
            _ => self.map(&parts, |c| builder.build_float_neg(c, "")),
        }
    }

    pub(crate) fn vector_binary(
        &self,
        op: BinOp,
        a: Reg,
        b: Reg,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let builder = &self.builder;
        let is_real = |reg| matches!(current.ty(reg), Ty::Prim(Type::Real));

        if is_real(b) || is_real(a) {
            let (vector, scale) = match is_real(b) {
                true => (a, current.value(b).into_float_value()),
                false => (b, current.value(a).into_float_value()),
            };
            let parts = self.components(current.value(vector).into_vector_value())?;
            return match op {
                BinOp::Mul => self.map(&parts, |c| builder.build_float_mul(c, scale, "")),
                BinOp::Div => self.map(&parts, |c| builder.build_float_div(c, scale, "")),
                other => unreachable!("the checker rejected {other:?} of a vector and a real"),
            };
        }

        let left = self.components(current.value(a).into_vector_value())?;
        let right = self.components(current.value(b).into_vector_value())?;
        match op {
            BinOp::Eq => Ok(self.all_equal(&left, &right)?.into()),
            BinOp::Ne => {
                let equal = self.all_equal(&left, &right)?;
                Ok(builder.build_not(equal, "")?.into())
            }
            BinOp::Add | BinOp::Sub => {
                let mut sum = Vec::with_capacity(left.len());
                for (x, y) in left.iter().zip(&right) {
                    sum.push(match op {
                        BinOp::Add => builder.build_float_add(*x, *y, "")?,
                        _ => builder.build_float_sub(*x, *y, "")?,
                    });
                }
                Ok(self.vector_of(&sum)?.into())
            }
            BinOp::Mul | BinOp::Div => self.complex_product(op, &left, &right),
            other => unreachable!("the checker rejected {other:?} of vectors"),
        }
    }

    /// `a == b` for every component, `false` if any of them is `NaN`.
    fn all_equal(
        &self,
        left: &[FloatValue<'ctx>],
        right: &[FloatValue<'ctx>],
    ) -> Result<IntValue<'ctx>> {
        let builder = &self.builder;
        let mut all = self.context.bool_type().const_all_ones();
        for (x, y) in left.iter().zip(right) {
            let equal = builder.build_float_compare(FloatPredicate::OEQ, *x, *y, "")?;
            all = builder.build_and(all, equal, "")?;
        }
        Ok(all)
    }

    /// The same formulas as `complex_op` in `ops.rs`.
    fn complex_product(
        &self,
        op: BinOp,
        left: &[FloatValue<'ctx>],
        right: &[FloatValue<'ctx>],
    ) -> Result<BasicValueEnum<'ctx>> {
        let builder = &self.builder;
        let (a, b, c, d) = (left[0], left[1], right[0], right[1]);
        let mul = |x, y| builder.build_float_mul(x, y, "");
        let result = match op {
            BinOp::Mul => [
                builder.build_float_sub(mul(a, c)?, mul(b, d)?, "")?,
                builder.build_float_add(mul(a, d)?, mul(b, c)?, "")?,
            ],
            _ => {
                let den = builder.build_float_add(mul(c, c)?, mul(d, d)?, "")?;
                let re = builder.build_float_add(mul(a, c)?, mul(b, d)?, "")?;
                let im = builder.build_float_sub(mul(b, c)?, mul(a, d)?, "")?;
                [
                    builder.build_float_div(re, den, "")?,
                    builder.build_float_div(im, den, "")?,
                ]
            }
        };
        Ok(self.vector_of(&result)?.into())
    }

    /// `v:zyx`. One letter gives a `real`, more give a vector.
    pub(crate) fn swizzle(
        &self,
        base: Reg,
        mask: &str,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let vector = current.value(base).into_vector_value();
        let picked = mask
            .chars()
            .map(|letter| {
                let position = "xyzw".find(letter).expect("checked swizzle letter");
                self.component(vector, position as u64)
            })
            .collect::<Result<Vec<_>>>()?;
        match picked.as_slice() {
            [single] => Ok((*single).into()),
            several => Ok(self.vector_of(several)?.into()),
        }
    }

    /// Built-ins that only assemble or take apart a vector register.
    pub(crate) fn vector_builtin(
        &self,
        name: &str,
        args: &[Reg],
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        match name {
            "complex" | "vec2" | "vec3" | "vec4" => {
                let parts: Vec<FloatValue> = args
                    .iter()
                    .map(|reg| current.value(*reg).into_float_value())
                    .collect();
                Ok(Some(self.vector_of(&parts)?.into()))
            }
            "re" | "im" => {
                let vector = current.value(args[0]).into_vector_value();
                let position = u64::from(name == "im");
                Ok(Some(self.component(vector, position)?.into()))
            }
            _ => Ok(None),
        }
    }
}
