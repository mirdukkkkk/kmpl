use std::sync::Arc;

use kmpl_ast::{BinOp, Span, Ty, Type, UnOp};
use kmpl_builtins::{Builtin, Io, StructLayout, StructValue, Value};
use kmpl_ir::{Block, Constant, FuncId, Function, Module, Op, Reg, Step, Target, Terminator};

use crate::{RuntimeError, ops};

/// Deepest allowed chain of function calls. Deeper recursion stops with an
/// error instead of using up all memory.
pub const MAX_CALL_DEPTH: usize = 10_000;

type Result<T> = std::result::Result<T, RuntimeError>;

fn error(message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError {
        message: message.into(),
        span,
    }
}

/// What a dead register is reset to. Holds nothing to release.
const EMPTY: Value = Value::Int(0);

/// A function waiting for a call it made to return.
struct Frame<'m> {
    function: &'m Function,
    block: &'m Block,
    next_inst: usize,
    base: usize,
    dest: Option<Reg>,
}

struct Vm<'m, 'o> {
    module: &'m Module,
    builtins: Vec<&'static Builtin>,
    layouts: Vec<Arc<StructLayout>>,
    globals: Vec<Value>,
    io: Io<'o>,
    /// The registers of every active call, one after another.
    registers: Vec<Value>,
    frames: Vec<Frame<'m>>,
    /// Reused for call arguments and jump arguments.
    scratch: Vec<Value>,
}

pub fn run(module: &Module, io: Io) -> std::result::Result<(), RuntimeError> {
    let module = without_needless_frees(module);
    let mut vm = Vm {
        module: &module,
        builtins: module
            .builtins
            .iter()
            .map(|name| kmpl_builtins::lookup(name).expect("lowering found this built-in"))
            .collect(),
        layouts: module
            .structs
            .iter()
            .map(|s| {
                Arc::new(StructLayout {
                    name: s.name.clone(),
                    fields: s.fields.clone(),
                })
            })
            .collect(),
        globals: vec![EMPTY; module.globals],
        io,
        registers: Vec::new(),
        frames: Vec::new(),
        scratch: Vec::new(),
    };

    vm.execute(module.init)?;
    vm.execute(module.main)
}

/// Only arrays, strings, tuples and structs hold anything to release.
/// Dropping the rest from the free lists saves work on every instruction.
fn without_needless_frees(module: &Module) -> Module {
    fn holds_memory(types: &[Ty], reg: Reg) -> bool {
        !matches!(
            types[reg.0 as usize],
            Ty::Prim(
                Type::Int
                    | Type::Real
                    | Type::Bool
                    | Type::Complex
                    | Type::Vec2
                    | Type::Vec3
                    | Type::Vec4
            )
        )
    }

    let mut module = module.clone();
    for function in &mut module.functions {
        let types = &function.types;
        for block in &mut function.blocks {
            block.term_frees.retain(|reg| holds_memory(types, *reg));
            for inst in &mut block.insts {
                inst.frees.retain(|reg| holds_memory(types, *reg));
            }
        }
    }
    module
}

fn as_int(value: &Value) -> i64 {
    match value {
        Value::Int(v) => *v,
        other => panic!("expected an int, got {other:?}, the checker should have rejected it"),
    }
}

fn as_bool(value: &Value) -> bool {
    match value {
        Value::Bool(v) => *v,
        other => panic!("expected a bool, got {other:?}, the checker should have rejected it"),
    }
}

/// `i` as a position in an array of length `len`.
fn checked_index(i: i64, len: usize, span: Span) -> Result<usize> {
    match usize::try_from(i) {
        Ok(position) if position < len => Ok(position),
        _ => Err(error(
            format!("index {i} is out of bounds for an array of length {len}"),
            span,
        )),
    }
}

/// The range `from..to` of an array of length `len`, or `from..=to`.
fn slice_bounds(
    from: i64,
    to: i64,
    inclusive: bool,
    len: usize,
) -> std::result::Result<std::ops::Range<usize>, String> {
    let bounds = (|| {
        let start = usize::try_from(from).ok()?;
        let end = usize::try_from(to).ok()?;
        let end = if inclusive { end.checked_add(1)? } else { end };
        (start <= end && end <= len).then_some(start..end)
    })();
    let dots = if inclusive { "..=" } else { ".." };
    bounds.ok_or_else(|| {
        format!("slice {from}{dots}{to} is out of bounds for an array of length {len}")
    })
}

fn constant(value: &Constant) -> Value {
    match value {
        Constant::Int(v) => Value::Int(*v),
        Constant::Real(v) => Value::Real(*v),
        Constant::Bool(v) => Value::Bool(*v),
        Constant::Complex { re, im } => Value::Complex { re: *re, im: *im },
        Constant::Str(text) => Value::Str(text.as_str().into()),
    }
}

/// Follows `path` from `root` and replaces what it finds. `Arc::make_mut`
/// copies an array or struct only if it is shared, which is what makes
/// them behave like values.
fn write_path(
    root: &mut Value,
    path: &[Step],
    registers: &[Value],
    base: usize,
    value: Value,
) -> Result<()> {
    let mut target = root;
    for step in path {
        target = match (step, target) {
            (Step::Index(index, span), Value::Array(items)) => {
                let items = Arc::make_mut(items);
                let i = as_int(&registers[base + index.0 as usize]);
                let i = checked_index(i, items.len(), *span)?;
                &mut items[i]
            }
            (Step::Field(index), Value::Struct(s)) => &mut Arc::make_mut(s).values[*index],
            (_, other) => panic!("bad target {other:?}, the checker should have rejected it"),
        };
    }
    *target = value;
    Ok(())
}

fn read_path(root: &Value, path: &[Step], registers: &[Value], base: usize) -> Result<Value> {
    let mut value = root.clone();
    for step in path {
        let element = match (step, &value) {
            (Step::Index(index, span), Value::Array(items)) => {
                let i = as_int(&registers[base + index.0 as usize]);
                items[checked_index(i, items.len(), *span)?].clone()
            }
            (Step::Field(index), Value::Struct(s)) => s.values[*index].clone(),
            (_, other) => panic!("bad target {other:?}, the checker should have rejected it"),
        };
        value = element;
    }
    Ok(value)
}

impl<'m> Vm<'m, '_> {
    fn execute(&mut self, start: FuncId) -> Result<()> {
        let module = self.module;
        let mut function = &module.functions[start.0 as usize];
        let mut base = self.registers.len();
        self.registers.resize(base + function.types.len(), EMPTY);
        let mut block = &function.blocks[0];
        let mut next_inst = 0;

        loop {
            let Some(inst) = block.insts.get(next_inst) else {
                match &block.term {
                    Terminator::Jump(target) => {
                        block = self.enter(function, target, base, &block.term_frees);
                        next_inst = 0;
                    }
                    Terminator::Branch {
                        cond,
                        then,
                        otherwise,
                    } => {
                        let target = if as_bool(&self.registers[base + cond.0 as usize]) {
                            then
                        } else {
                            otherwise
                        };
                        block = self.enter(function, target, base, &block.term_frees);
                        next_inst = 0;
                    }
                    Terminator::Return(value) => {
                        let result = value.map(|reg| {
                            std::mem::replace(&mut self.registers[base + reg.0 as usize], EMPTY)
                        });
                        self.registers.truncate(base);

                        let Some(frame) = self.frames.pop() else {
                            return Ok(());
                        };
                        function = frame.function;
                        block = frame.block;
                        next_inst = frame.next_inst;
                        base = frame.base;
                        if let (Some(dest), Some(result)) = (frame.dest, result) {
                            self.registers[base + dest.0 as usize] = result;
                        }
                    }
                    Terminator::Unreachable => unreachable!("this block never runs"),
                }
                continue;
            };
            next_inst += 1;

            if let Op::Call { func, args } = &inst.op {
                if self.frames.len() + 1 >= MAX_CALL_DEPTH {
                    return Err(error(
                        format!("stack overflow: more than {MAX_CALL_DEPTH} nested calls"),
                        inst.span,
                    ));
                }

                let callee = &module.functions[func.0 as usize];
                let new_base = self.registers.len();
                self.registers.resize(new_base + callee.types.len(), EMPTY);
                for (param, arg) in callee.params.iter().zip(args) {
                    let value = self.registers[base + arg.0 as usize].clone();
                    self.registers[new_base + param.0 as usize] = value;
                }
                // Before the call, so the callee can own these values.
                for reg in &inst.frees {
                    self.registers[base + reg.0 as usize] = EMPTY;
                }

                self.frames.push(Frame {
                    function,
                    block,
                    next_inst,
                    base,
                    dest: inst.dest,
                });
                function = callee;
                block = &callee.blocks[0];
                next_inst = 0;
                base = new_base;
                continue;
            }

            // The instructions every loop runs stay out of the general path.
            let result = match &inst.op {
                Op::Binary { op, a, b } => Some(self.binary(*op, *a, *b, base, inst.span)?),
                Op::Const(value) => Some(constant(value)),
                Op::Builtin { id, args } => {
                    self.builtin(*id, args, &inst.frees, base, inst.span)?
                }
                Op::RangeContinues {
                    value,
                    end,
                    step,
                    inclusive,
                } => {
                    let (v, end) = (as_int(self.reg(base, *value)), as_int(self.reg(base, *end)));
                    Some(Value::Bool(
                        match (as_int(self.reg(base, *step)) > 0, inclusive) {
                            (true, false) => v < end,
                            (true, true) => v <= end,
                            (false, false) => v > end,
                            (false, true) => v >= end,
                        },
                    ))
                }
                Op::AddOverflows { a, b } => {
                    let (a, b) = (as_int(self.reg(base, *a)), as_int(self.reg(base, *b)));
                    Some(Value::Bool(a.checked_add(b).is_none()))
                }
                other => self.operation(other, base, inst.span)?,
            };
            if let (Some(dest), Some(result)) = (inst.dest, result) {
                self.registers[base + dest.0 as usize] = result;
            }
            for reg in &inst.frees {
                self.registers[base + reg.0 as usize] = EMPTY;
            }
        }
    }

    /// Passes the arguments of a jump and returns the block it goes to.
    #[inline]
    fn enter(
        &mut self,
        function: &'m Function,
        target: &Target,
        base: usize,
        frees: &[Reg],
    ) -> &'m Block {
        let block = &function.blocks[target.block.0 as usize];

        // An argument that is also a parameter of the target (a jump that
        // swaps two of them) has to be read before anything is written.
        if target.args.iter().any(|arg| block.params.contains(arg)) {
            self.scratch.clear();
            for arg in &target.args {
                self.scratch
                    .push(self.registers[base + arg.0 as usize].clone());
            }
            for reg in frees {
                self.registers[base + reg.0 as usize] = EMPTY;
            }
            for (param, value) in block.params.iter().zip(self.scratch.drain(..)) {
                self.registers[base + param.0 as usize] = value;
            }
            return block;
        }

        for (param, arg) in block.params.iter().zip(&target.args) {
            let value = self.registers[base + arg.0 as usize].clone();
            self.registers[base + param.0 as usize] = value;
        }
        for reg in frees {
            self.registers[base + reg.0 as usize] = EMPTY;
        }
        block
    }

    #[inline]
    fn reg(&self, base: usize, reg: Reg) -> &Value {
        &self.registers[base + reg.0 as usize]
    }

    fn collect(&self, base: usize, regs: &[Reg]) -> Vec<Value> {
        regs.iter().map(|r| self.reg(base, *r).clone()).collect()
    }

    /// Runs everything except a call. `None` for operations with no result.
    fn operation(&mut self, op: &Op, base: usize, span: Span) -> Result<Option<Value>> {
        let value = match op {
            Op::Unary { op, a } => match (op, self.reg(base, *a)) {
                (UnOp::Neg, Value::Real(x)) => Value::Real(-x),
                (UnOp::Not, Value::Bool(x)) => Value::Bool(!x),
                (_, other) => ops::unary(*op, other.clone()).map_err(|m| error(m, span))?,
            },
            Op::Call { .. }
            | Op::Binary { .. }
            | Op::Const(_)
            | Op::Builtin { .. }
            | Op::RangeContinues { .. }
            | Op::AddOverflows { .. } => unreachable!("handled by `execute`"),
            Op::GetGlobal(id) => self.globals[*id].clone(),
            Op::SetGlobal(id, value) => {
                self.globals[*id] = self.reg(base, *value).clone();
                return Ok(None);
            }
            Op::MakeArray(items) => Value::Array(Arc::new(self.collect(base, items))),
            Op::ArrayFill { value, len } => self.array_fill(*value, *len, base, span)?,
            Op::ArrayLen(array) => Value::Int(self.reg(base, *array).as_array().len() as i64),
            Op::Index { base: array, index } => {
                let items = self.reg(base, *array).as_array();
                let i = as_int(self.reg(base, *index));
                items[checked_index(i, items.len(), span)?].clone()
            }
            Op::Slice {
                base: array,
                start,
                end,
                inclusive,
            } => {
                let items = self.reg(base, *array).as_array();
                let from = start.map_or(0, |r| as_int(self.reg(base, r)));
                let (to, inclusive) = match end {
                    Some(end) => (as_int(self.reg(base, *end)), *inclusive),
                    None => (items.len() as i64, false),
                };
                let range =
                    slice_bounds(from, to, inclusive, items.len()).map_err(|m| error(m, span))?;
                Value::Array(Arc::new(items[range].to_vec()))
            }
            Op::MakeTuple(items) => Value::Tuple(self.collect(base, items).into()),
            Op::TupleGet { tuple, index } => match self.reg(base, *tuple) {
                Value::Tuple(items) => items[*index].clone(),
                other => panic!("unpacking {other:?}, the checker should have rejected it"),
            },
            Op::MakeStruct { id, values } => Value::Struct(Arc::new(StructValue {
                layout: self.layouts[*id].clone(),
                values: self.collect(base, values),
            })),
            Op::FieldGet { base: value, index } => match self.reg(base, *value) {
                Value::Struct(s) => s.values[*index].clone(),
                other => panic!("a field of {other:?}, the checker should have rejected it"),
            },
            Op::PathGet { base: root, path } => {
                read_path(self.reg(base, *root), path, &self.registers, base)?
            }
            Op::PathSet {
                base: root,
                path,
                value,
                take,
            } => {
                let slot = base + root.0 as usize;
                let mut updated = if *take {
                    std::mem::replace(&mut self.registers[slot], EMPTY)
                } else {
                    self.registers[slot].clone()
                };
                let value = self.reg(base, *value).clone();
                write_path(&mut updated, path, &self.registers, base, value)?;
                updated
            }
            Op::Swizzle { base: value, mask } => ops::swizzle(self.reg(base, *value).clone(), mask),
            Op::CheckStep(step) => {
                if as_int(self.reg(base, *step)) == 0 {
                    return Err(error("loop step cannot be zero", span));
                }
                return Ok(None);
            }
        };
        Ok(Some(value))
    }

    #[inline]
    fn binary(&self, op: BinOp, a: Reg, b: Reg, base: usize, span: Span) -> Result<Value> {
        let (left, right) = (self.reg(base, a), self.reg(base, b));

        // The common cases, without cloning or a general dispatch.
        if !matches!(op, BinOp::Eq | BinOp::Ne) {
            match (left, right) {
                (Value::Real(x), Value::Real(y)) => {
                    if let Some(value) = ops::real_op(op, *x, *y) {
                        return Ok(value);
                    }
                }
                (Value::Int(x), Value::Int(y)) => {
                    return ops::int_op(op, *x, *y).map_err(|m| error(m, span));
                }
                _ => {}
            }
        }
        ops::binary(op, left.clone(), right.clone()).map_err(|m| error(m, span))
    }

    /// An argument used for the last time is moved, so the built-in can
    /// reuse it. The rest are shared.
    fn builtin(
        &mut self,
        id: usize,
        args: &[Reg],
        frees: &[Reg],
        base: usize,
        span: Span,
    ) -> Result<Option<Value>> {
        let mut values = std::mem::take(&mut self.scratch);
        values.clear();
        for (position, reg) in args.iter().enumerate() {
            let slot = base + reg.0 as usize;
            let last_use = frees.contains(reg) && !args[position + 1..].contains(reg);
            values.push(if last_use {
                std::mem::replace(&mut self.registers[slot], EMPTY)
            } else {
                self.registers[slot].clone()
            });
        }

        let result = (self.builtins[id].func)(&mut values, &mut self.io);

        // Holding on to the arguments would make a later write to one of
        // those arrays copy it.
        values.clear();
        self.scratch = values;
        result.map_err(|m| error(m, span))
    }

    fn array_fill(&self, value: Reg, len: Reg, base: usize, span: Span) -> Result<Value> {
        let item = self.reg(base, value).clone();
        let n = as_int(self.reg(base, len));
        let count = usize::try_from(n)
            .map_err(|_| error(format!("array length cannot be negative, got {n}"), span))?;

        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| error(format!("an array of {n} elements is too large"), span))?;
        values.resize(count, item);
        Ok(Value::Array(Arc::new(values)))
    }
}
