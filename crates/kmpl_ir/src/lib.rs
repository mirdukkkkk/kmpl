//! The intermediate representation between the checked program and its
//! execution.
//!
//! A function is a list of basic blocks in SSA form: every register is
//! assigned exactly once, and arrays and structs are updated by making a
//! new register (`PathSet`). Values that come from several paths arrive as
//! block parameters, which are equivalent to phi functions.

mod display;
mod liveness;
mod opt;

#[cfg(test)]
mod tests;

pub use liveness::annotate;
pub use opt::optimize;

use kmpl_ast::{BinOp, Span, Ty, UnOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Reg(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FuncId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub functions: Vec<Function>,
    /// Runs before `main` and sets every global: the `const` items.
    pub init: FuncId,
    pub main: FuncId,
    pub globals: usize,
    /// Names of the built-ins that `Op::Builtin` refers to.
    pub builtins: Vec<String>,
    pub structs: Vec<StructLayout>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructLayout {
    pub name: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    /// The parameters of the first block.
    pub params: Vec<Reg>,
    pub ret: Option<Ty>,
    pub blocks: Vec<Block>,
    /// The type of every register, by number.
    pub types: Vec<Ty>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub params: Vec<Reg>,
    pub insts: Vec<Inst>,
    pub term: Terminator,
    /// Registers used for the last time by the terminator.
    pub term_frees: Vec<Reg>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inst {
    pub dest: Option<Reg>,
    pub op: Op,
    /// Where a runtime error in this instruction is reported.
    pub span: Span,
    /// Registers not used after this instruction, set by `annotate`.
    pub frees: Vec<Reg>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(i64),
    Real(f64),
    Bool(bool),
    Complex { re: f64, im: f64 },
    Str(String),
}

/// One step of a path into an array or struct.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// The span is where a bad index is reported.
    Index(Reg, Span),
    Field(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Const(Constant),
    Unary {
        op: UnOp,
        a: Reg,
    },
    Binary {
        op: BinOp,
        a: Reg,
        b: Reg,
    },
    Call {
        func: FuncId,
        args: Vec<Reg>,
    },
    /// `id` is an index into `Module::builtins`.
    Builtin {
        id: usize,
        args: Vec<Reg>,
    },
    GetGlobal(usize),
    SetGlobal(usize, Reg),
    MakeArray(Vec<Reg>),
    ArrayFill {
        value: Reg,
        len: Reg,
    },
    ArrayLen(Reg),
    Index {
        base: Reg,
        index: Reg,
    },
    Slice {
        base: Reg,
        start: Option<Reg>,
        end: Option<Reg>,
        inclusive: bool,
    },
    MakeTuple(Vec<Reg>),
    TupleGet {
        tuple: Reg,
        index: usize,
    },
    /// `id` is an index into `Module::structs`, `values` follow its fields.
    MakeStruct {
        id: usize,
        values: Vec<Reg>,
    },
    FieldGet {
        base: Reg,
        index: usize,
    },
    PathGet {
        base: Reg,
        path: Vec<Step>,
    },
    /// A copy of `base` with the part at `path` replaced. `take` is set by
    /// `annotate` when `base` is not used afterwards, so it can be updated
    /// in place instead of being copied.
    PathSet {
        base: Reg,
        path: Vec<Step>,
        value: Reg,
        take: bool,
    },
    Swizzle {
        base: Reg,
        mask: String,
    },
    /// Stops the program if a loop's `step` is zero.
    CheckStep(Reg),
    /// Is `value` still inside the range ending at `end`?
    RangeContinues {
        value: Reg,
        end: Reg,
        step: Reg,
        inclusive: bool,
    },
    /// Would `a + b` not fit an `int`?
    AddOverflows {
        a: Reg,
        b: Reg,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub block: BlockId,
    pub args: Vec<Reg>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    Jump(Target),
    Branch {
        cond: Reg,
        then: Target,
        otherwise: Target,
    },
    Return(Option<Reg>),
    /// A block still being built, or one that can never run.
    Unreachable,
}

impl Op {
    /// Calls `f` with every register this operation reads.
    pub fn for_each_use(&self, mut f: impl FnMut(Reg)) {
        fn steps(path: &[Step], f: &mut impl FnMut(Reg)) {
            for step in path {
                if let Step::Index(reg, _) = step {
                    f(*reg);
                }
            }
        }

        match self {
            Op::Const(_) | Op::GetGlobal(_) => {}
            Op::Unary { a, .. } | Op::ArrayLen(a) | Op::CheckStep(a) | Op::SetGlobal(_, a) => f(*a),
            Op::Binary { a, b, .. } | Op::AddOverflows { a, b } => {
                f(*a);
                f(*b);
            }
            Op::Call { args, .. }
            | Op::Builtin { args, .. }
            | Op::MakeArray(args)
            | Op::MakeTuple(args)
            | Op::MakeStruct { values: args, .. } => args.iter().copied().for_each(f),
            Op::ArrayFill { value, len } => {
                f(*value);
                f(*len);
            }
            Op::Index { base, index } => {
                f(*base);
                f(*index);
            }
            Op::Slice {
                base, start, end, ..
            } => {
                f(*base);
                for bound in [start, end].into_iter().flatten() {
                    f(*bound);
                }
            }
            Op::TupleGet { tuple: base, .. }
            | Op::FieldGet { base, .. }
            | Op::Swizzle { base, .. } => f(*base),
            Op::PathGet { base, path } => {
                f(*base);
                steps(path, &mut f);
            }
            Op::PathSet {
                base, path, value, ..
            } => {
                f(*base);
                steps(path, &mut f);
                f(*value);
            }
            Op::RangeContinues {
                value, end, step, ..
            } => {
                f(*value);
                f(*end);
                f(*step);
            }
        }
    }
}

impl Op {
    /// Calls `f` with a mutable reference to every register this operation
    /// reads, so a pass can replace them.
    pub fn for_each_use_mut(&mut self, mut f: impl FnMut(&mut Reg)) {
        fn steps(path: &mut [Step], f: &mut impl FnMut(&mut Reg)) {
            for step in path {
                if let Step::Index(reg, _) = step {
                    f(reg);
                }
            }
        }

        match self {
            Op::Const(_) | Op::GetGlobal(_) => {}
            Op::Unary { a, .. } | Op::ArrayLen(a) | Op::CheckStep(a) | Op::SetGlobal(_, a) => f(a),
            Op::Binary { a, b, .. } | Op::AddOverflows { a, b } => {
                f(a);
                f(b);
            }
            Op::Call { args, .. }
            | Op::Builtin { args, .. }
            | Op::MakeArray(args)
            | Op::MakeTuple(args)
            | Op::MakeStruct { values: args, .. } => args.iter_mut().for_each(f),
            Op::ArrayFill { value, len } => {
                f(value);
                f(len);
            }
            Op::Index { base, index } => {
                f(base);
                f(index);
            }
            Op::Slice {
                base, start, end, ..
            } => {
                f(base);
                for bound in [start, end].into_iter().flatten() {
                    f(bound);
                }
            }
            Op::TupleGet { tuple: base, .. }
            | Op::FieldGet { base, .. }
            | Op::Swizzle { base, .. } => f(base),
            Op::PathGet { base, path } => {
                f(base);
                steps(path, &mut f);
            }
            Op::PathSet {
                base, path, value, ..
            } => {
                f(base);
                steps(path, &mut f);
                f(value);
            }
            Op::RangeContinues {
                value, end, step, ..
            } => {
                f(value);
                f(end);
                f(step);
            }
        }
    }
}

impl Terminator {
    /// The jump targets.
    pub fn targets(&self) -> Vec<&Target> {
        match self {
            Terminator::Jump(target) => vec![target],
            Terminator::Branch {
                then, otherwise, ..
            } => vec![then, otherwise],
            Terminator::Return(_) | Terminator::Unreachable => Vec::new(),
        }
    }

    /// The jump targets, to change them.
    pub fn targets_mut(&mut self) -> Vec<&mut Target> {
        match self {
            Terminator::Jump(target) => vec![target],
            Terminator::Branch {
                then, otherwise, ..
            } => vec![then, otherwise],
            Terminator::Return(_) | Terminator::Unreachable => Vec::new(),
        }
    }

    /// Calls `f` with a mutable reference to every register this terminator
    /// reads.
    pub fn for_each_use_mut(&mut self, mut f: impl FnMut(&mut Reg)) {
        match self {
            Terminator::Jump(target) => target.args.iter_mut().for_each(f),
            Terminator::Branch {
                cond,
                then,
                otherwise,
            } => {
                f(cond);
                then.args.iter_mut().for_each(&mut f);
                otherwise.args.iter_mut().for_each(f);
            }
            Terminator::Return(value) => value.iter_mut().for_each(f),
            Terminator::Unreachable => {}
        }
    }

    /// Calls `f` with every register this terminator reads.
    pub fn for_each_use(&self, mut f: impl FnMut(Reg)) {
        match self {
            Terminator::Jump(target) => target.args.iter().copied().for_each(f),
            Terminator::Branch {
                cond,
                then,
                otherwise,
            } => {
                f(*cond);
                then.args.iter().copied().for_each(&mut f);
                otherwise.args.iter().copied().for_each(f);
            }
            Terminator::Return(value) => value.iter().copied().for_each(f),
            Terminator::Unreachable => {}
        }
    }

    /// The blocks control can go to next.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Jump(target) => vec![target.block],
            Terminator::Branch {
                then, otherwise, ..
            } => vec![then.block, otherwise.block],
            Terminator::Return(_) | Terminator::Unreachable => Vec::new(),
        }
    }
}
