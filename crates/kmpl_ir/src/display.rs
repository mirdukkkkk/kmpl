//! The text form printed by `kmpl --ir`.

use std::fmt::{self, Display, Formatter};

use kmpl_ast::{BinOp, Ty, Type, UnOp};

use crate::{Block, BlockId, Constant, Function, Inst, Module, Op, Reg, Step, Target, Terminator};

impl Display for Reg {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

impl Display for BlockId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "b{}", self.0)
    }
}

fn list(regs: &[Reg]) -> String {
    let parts: Vec<String> = regs.iter().map(Reg::to_string).collect();
    parts.join(", ")
}

fn type_text(ty: &Ty) -> String {
    match ty {
        Ty::Prim(Type::Int) => "int".to_owned(),
        Ty::Prim(Type::Real) => "real".to_owned(),
        Ty::Prim(Type::Bool) => "bool".to_owned(),
        Ty::Prim(Type::Complex) => "complex".to_owned(),
        Ty::Prim(Type::Vec2) => "vec2".to_owned(),
        Ty::Prim(Type::Vec3) => "vec3".to_owned(),
        Ty::Prim(Type::Vec4) => "vec4".to_owned(),
        Ty::Prim(Type::Str) => "string".to_owned(),
        Ty::Prim(Type::Bytes) => "bytes".to_owned(),
        Ty::Array(elem) => format!("[{}]", type_text(elem)),
        Ty::Tuple(items) => {
            let parts: Vec<String> = items.iter().map(type_text).collect();
            format!("({})", parts.join(", "))
        }
        Ty::Struct(name) => name.clone(),
        Ty::Optional(inner) => format!("{}?", type_text(inner)),
    }
}

fn binary_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::Rem => "rem",
        BinOp::BitAnd => "bitand",
        BinOp::BitOr => "bitor",
        BinOp::BitXor => "bitxor",
        BinOp::Shl => "shl",
        BinOp::Shr => "shr",
        BinOp::Eq => "eq",
        BinOp::Ne => "ne",
        BinOp::Lt => "lt",
        BinOp::Le => "le",
        BinOp::Gt => "gt",
        BinOp::Ge => "ge",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

fn path_text(path: &[Step]) -> String {
    let parts: Vec<String> = path
        .iter()
        .map(|step| match step {
            Step::Index(reg, _) => format!("[{reg}]"),
            Step::Field(index) => format!(".{index}"),
        })
        .collect();
    parts.concat()
}

fn target_text(target: &Target) -> String {
    if target.args.is_empty() {
        target.block.to_string()
    } else {
        format!("{}({})", target.block, list(&target.args))
    }
}

impl Module {
    fn op_text(&self, op: &Op) -> String {
        match op {
            Op::Const(Constant::Int(v)) => format!("const {v}"),
            Op::Const(Constant::Real(v)) => format!("const {v:?}"),
            Op::Const(Constant::Bool(v)) => format!("const {v}"),
            Op::Const(Constant::Complex { re, im }) => format!("const {re:?}{im:+?}i"),
            Op::Const(Constant::Str(text)) => format!("const {text:?}"),
            Op::Unary { op: UnOp::Neg, a } => format!("neg {a}"),
            Op::Unary { op: UnOp::Not, a } => format!("not {a}"),
            Op::Unary {
                op: UnOp::BitNot,
                a,
            } => format!("bitnot {a}"),
            Op::Binary { op, a, b } => format!("{} {a}, {b}", binary_name(*op)),
            Op::Call { func, args } => {
                let name = &self.functions[func.0 as usize].name;
                format!("call {name}({})", list(args))
            }
            Op::Builtin { id, args } => {
                format!("builtin {}({})", self.builtins[*id], list(args))
            }
            Op::GetGlobal(id) => format!("global {id}"),
            Op::SetGlobal(id, value) => format!("set_global {id}, {value}"),
            Op::MakeArray(items) => format!("array [{}]", list(items)),
            Op::ArrayFill { value, len } => format!("fill {value}, {len}"),
            Op::ArrayLen(array) => format!("array_len {array}"),
            Op::Index { base, index } => format!("index {base}, {index}"),
            Op::Slice {
                base,
                start,
                end,
                inclusive,
            } => {
                let bound = |b: &Option<Reg>| b.map_or(String::new(), |r| r.to_string());
                let dots = if *inclusive { "..=" } else { ".." };
                format!("slice {base}[{}{dots}{}]", bound(start), bound(end))
            }
            Op::MakeTuple(items) => format!("tuple ({})", list(items)),
            Op::TupleGet { tuple, index } => format!("tuple_get {tuple}, {index}"),
            Op::MakeStruct { id, values } => {
                format!("struct {}({})", self.structs[*id].name, list(values))
            }
            Op::FieldGet { base, index } => format!("field {base}, {index}"),
            Op::MakeEnum {
                id,
                variant,
                values,
            } => {
                let layout = &self.structs[*id];
                let name = &layout.fields[*variant];
                format!("enum {}::{name}({})", layout.name, list(values))
            }
            Op::MakeSome(value) => format!("some {value}"),
            Op::MakeNone => "none".to_owned(),
            Op::VariantOf(base) => format!("variant_of {base}"),
            Op::VariantField { base, index } => format!("variant_field {base}, {index}"),
            Op::PathGet { base, path } => format!("get {base}{}", path_text(path)),
            Op::PathSet {
                base, path, value, ..
            } => format!("set {base}{}, {value}", path_text(path)),
            Op::Swizzle { base, mask } => format!("swizzle {base}, {mask}"),
            Op::CheckStep(step) => format!("check_step {step}"),
            Op::RangeContinues {
                value,
                end,
                step,
                inclusive,
            } => {
                let name = if *inclusive {
                    "in_range_inclusive"
                } else {
                    "in_range"
                };
                format!("{name} {value}, {end}, {step}")
            }
            Op::AddOverflows { a, b } => format!("add_overflows {a}, {b}"),
        }
    }

    fn write_inst(&self, f: &mut Formatter<'_>, inst: &Inst) -> fmt::Result {
        match inst.dest {
            Some(dest) => writeln!(f, "    {dest} = {}", self.op_text(&inst.op)),
            None => writeln!(f, "    {}", self.op_text(&inst.op)),
        }
    }

    fn write_block(
        &self,
        f: &mut Formatter<'_>,
        function: &Function,
        index: usize,
        block: &Block,
    ) -> fmt::Result {
        if block.params.is_empty() || index == 0 {
            writeln!(f, "b{index}:")?;
        } else {
            let params: Vec<String> = block
                .params
                .iter()
                .map(|reg| format!("{reg}: {}", type_text(&function.types[reg.0 as usize])))
                .collect();
            writeln!(f, "b{index}({}):", params.join(", "))?;
        }

        for inst in &block.insts {
            self.write_inst(f, inst)?;
        }

        match &block.term {
            Terminator::Jump(target) => writeln!(f, "    jump {}", target_text(target)),
            Terminator::Branch {
                cond,
                then,
                otherwise,
            } => writeln!(
                f,
                "    br {cond}, {}, {}",
                target_text(then),
                target_text(otherwise)
            ),
            Terminator::Return(Some(value)) => writeln!(f, "    ret {value}"),
            Terminator::Return(None) => writeln!(f, "    ret"),
            Terminator::Unreachable => writeln!(f, "    unreachable"),
        }
    }
}

impl Display for Module {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (i, function) in self.functions.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            let params: Vec<String> = function
                .params
                .iter()
                .map(|reg| format!("{reg}: {}", type_text(&function.types[reg.0 as usize])))
                .collect();
            let ret = function
                .ret
                .as_ref()
                .map_or(String::new(), |ty| format!(" -> {}", type_text(ty)));
            writeln!(f, "fn {}({}){ret} {{", function.name, params.join(", "))?;
            for (index, block) in function.blocks.iter().enumerate() {
                self.write_block(f, function, index, block)?;
            }
            writeln!(f, "}}")?;
        }
        Ok(())
    }
}
