//! Tree-walking evaluation of a checked program.
//!
//! The checker guarantees names, types, argument counts and definite
//! assignment, so this code does not re-check them; a violation is a
//! checker bug and panics.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use kmpl_ast::{
    BinOp, Block, Expr, ExprKind, Function, Ident, Item, Program, Span, Stmt, StmtKind,
};
use kmpl_builtins::Value;

use crate::RuntimeError;
use crate::ops;

/// Deepest allowed chain of user function calls. Deeper recursion stops
/// with an error instead of crashing the interpreter.
pub const MAX_CALL_DEPTH: usize = 10_000;

/// What a statement tells the enclosing code to do next.
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Option<Value>),
}

/// Variables of one function call. Innermost scope is last.
/// `None` is a variable declared with `let x;` and not assigned yet.
struct Frame<'p> {
    scopes: Vec<HashMap<&'p str, Option<Value>>>,
}

pub struct Interp<'p, 'o> {
    functions: HashMap<&'p str, &'p Function>,
    consts: HashMap<&'p str, Value>,
    frames: Vec<Frame<'p>>,
    out: &'o mut dyn Write,
}

type Result<T> = std::result::Result<T, RuntimeError>;

fn error(message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError {
        message: message.into(),
        span,
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

impl<'p, 'o> Interp<'p, 'o> {
    pub fn new(program: &'p Program, out: &'o mut dyn Write) -> Self {
        let functions = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(f) => Some((f.name.name.as_str(), f)),
                Item::Const(_) => None,
            })
            .collect();

        Self {
            functions,
            consts: HashMap::new(),
            frames: Vec::new(),
            out,
        }
    }

    /// Evaluates the constants in order, then calls `main`.
    pub fn run(&mut self, program: &'p Program) -> Result<()> {
        for item in &program.items {
            if let Item::Const(c) = item {
                let value = self.eval(&c.value)?;
                self.consts.insert(&c.name.name, value);
            }
        }

        let main = self.functions["main"];
        self.call_function(main, Vec::new(), main.name.span)?;
        Ok(())
    }

    // ----- variables -----

    fn frame(&mut self) -> &mut Frame<'p> {
        self.frames.last_mut().expect("inside a function")
    }

    fn declare(&mut self, name: &'p str, value: Option<Value>) {
        let scope = self.frame().scopes.last_mut().expect("inside a block");
        scope.insert(name, value);
    }

    fn read(&self, name: &Ident) -> Value {
        if let Some(frame) = self.frames.last() {
            for scope in frame.scopes.iter().rev() {
                if let Some(slot) = scope.get(name.name.as_str()) {
                    return slot.clone().expect("checker ensures assignment before use");
                }
            }
        }
        match self.consts.get(name.name.as_str()) {
            Some(value) => value.clone(),
            None => {
                let builtin = kmpl_builtins::lookup_const(&name.name);
                builtin
                    .expect("checker ensures the name exists")
                    .value
                    .clone()
            }
        }
    }

    fn slot(&mut self, name: &Ident) -> &mut Option<Value> {
        for scope in self.frame().scopes.iter_mut().rev() {
            if let Some(slot) = scope.get_mut(name.name.as_str()) {
                return slot;
            }
        }
        unreachable!("checker ensures `{}` is a local variable", name.name);
    }

    fn assign(&mut self, name: &Ident, value: Value) {
        *self.slot(name) = Some(value);
    }

    /// `name[i][j] = value`. `Arc::make_mut` copies an array only if it
    /// is shared, which is what makes arrays behave like values.
    fn assign_index(
        &mut self,
        name: &Ident,
        positions: &[(i64, Span)],
        value: Value,
    ) -> Result<()> {
        let slot = self.slot(name);
        let mut target = slot
            .as_mut()
            .expect("checker ensures assignment before use");
        for &(i, span) in positions {
            let Value::Array(items) = target else {
                panic!("indexing {target:?}, the checker should have rejected it");
            };
            let items = Arc::make_mut(items);
            let i = checked_index(i, items.len(), span)?;
            target = &mut items[i];
        }
        *target = value;
        Ok(())
    }

    // ----- statements -----

    fn block(&mut self, block: &'p Block) -> Result<Flow> {
        self.frame().scopes.push(HashMap::new());
        let mut flow = Flow::Normal;
        for stmt in block {
            flow = self.stmt(stmt)?;
            if !matches!(flow, Flow::Normal) {
                break;
            }
        }
        self.frame().scopes.pop();
        Ok(flow)
    }

    fn stmt(&mut self, stmt: &'p Stmt) -> Result<Flow> {
        match &stmt.kind {
            StmtKind::Let { name, init, .. } => {
                let value = match init {
                    Some(init) => Some(self.eval(init)?),
                    None => None,
                };
                self.declare(&name.name, value);
            }
            StmtKind::Assign { name, value } => {
                let value = self.eval(value)?;
                self.assign(name, value);
            }
            StmtKind::IndexAssign {
                name,
                indices,
                value,
            } => {
                let value = self.eval(value)?;
                let mut positions = Vec::with_capacity(indices.len());
                for index in indices {
                    positions.push((self.eval_int(index)?, index.span));
                }
                self.assign_index(name, &positions, value)?;
            }
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => {
                if self.eval_bool(cond)? {
                    return self.block(then);
                }
                if let Some(otherwise) = otherwise {
                    return self.block(otherwise);
                }
            }
            StmtKind::While { cond, body } => {
                while self.eval_bool(cond)? {
                    match self.block(body)? {
                        Flow::Break => break,
                        Flow::Normal | Flow::Continue => {}
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                    }
                }
            }
            StmtKind::For {
                var,
                start,
                end,
                body,
            } => {
                let start = self.eval_int(start)?;
                let end = self.eval_int(end)?;
                return self.for_loop(var, body, (start..end).map(Value::Int));
            }
            StmtKind::ForEach { var, array, body } => {
                // `items` keeps the array as it was when the loop started.
                let items = self.eval_array(array)?;
                return self.for_loop(var, body, items.iter().cloned());
            }
            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
            StmtKind::Return(value) => {
                let value = match value {
                    Some(value) => Some(self.eval(value)?),
                    None => None,
                };
                return Ok(Flow::Return(value));
            }
            StmtKind::Expr(e) => match &e.kind {
                // May be a call to a function that returns nothing.
                ExprKind::Call { name, args } => {
                    self.call(name, args, e.span)?;
                }
                _ => {
                    self.eval(e)?;
                }
            },
        }
        Ok(Flow::Normal)
    }

    /// Runs `body` once per value, with `var` set to it.
    fn for_loop(
        &mut self,
        var: &'p Ident,
        body: &'p Block,
        values: impl Iterator<Item = Value>,
    ) -> Result<Flow> {
        self.frame().scopes.push(HashMap::new());
        let mut result = Flow::Normal;
        for value in values {
            self.declare(&var.name, Some(value));
            match self.block(body)? {
                Flow::Break => break,
                Flow::Normal | Flow::Continue => {}
                flow @ Flow::Return(_) => {
                    result = flow;
                    break;
                }
            }
        }
        self.frame().scopes.pop();
        Ok(result)
    }

    // ----- expressions -----

    fn eval_bool(&mut self, e: &'p Expr) -> Result<bool> {
        match self.eval(e)? {
            Value::Bool(b) => Ok(b),
            other => panic!("condition is {other:?}, the checker should have rejected it"),
        }
    }

    fn eval_int(&mut self, e: &'p Expr) -> Result<i64> {
        match self.eval(e)? {
            Value::Int(v) => Ok(v),
            other => panic!("expected an int, got {other:?}, the checker should have rejected it"),
        }
    }

    fn eval_array(&mut self, e: &'p Expr) -> Result<Arc<Vec<Value>>> {
        match self.eval(e)? {
            Value::Array(items) => Ok(items),
            other => {
                panic!("expected an array, got {other:?}, the checker should have rejected it")
            }
        }
    }

    fn eval(&mut self, e: &'p Expr) -> Result<Value> {
        let value = match &e.kind {
            ExprKind::Int(v) => Value::Int(*v),
            ExprKind::Real(v) => Value::Real(*v),
            ExprKind::Complex { re, im } => Value::Complex { re: *re, im: *im },
            ExprKind::Bool(v) => Value::Bool(*v),
            ExprKind::Var(name) => self.read(name),
            ExprKind::Unary { op, operand } => {
                let v = self.eval(operand)?;
                ops::unary(*op, v).map_err(|m| error(m, e.span))?
            }
            ExprKind::Binary {
                op: op @ (BinOp::And | BinOp::Or),
                lhs,
                rhs,
            } => {
                // Short circuit: `false && x` and `true || x` skip `x`.
                let l = self.eval_bool(lhs)?;
                let skip = if *op == BinOp::And { !l } else { l };
                Value::Bool(if skip { l } else { self.eval_bool(rhs)? })
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.eval(lhs)?;
                let r = self.eval(rhs)?;
                ops::binary(*op, l, r).map_err(|m| error(m, e.span))?
            }
            ExprKind::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item)?);
                }
                Value::Array(Arc::new(values))
            }
            ExprKind::ArrayFill { value, len } => {
                let item = self.eval(value)?;
                let n = self.eval_int(len)?;
                let count = usize::try_from(n).map_err(|_| {
                    error(
                        format!("array length cannot be negative, got {n}"),
                        len.span,
                    )
                })?;
                let mut values = Vec::new();
                values.try_reserve_exact(count).map_err(|_| {
                    error(format!("an array of {n} elements is too large"), len.span)
                })?;
                values.resize(count, item);
                Value::Array(Arc::new(values))
            }
            ExprKind::Index { base, index } => {
                let items = self.eval_array(base)?;
                let i = self.eval_int(index)?;
                items[checked_index(i, items.len(), index.span)?].clone()
            }
            ExprKind::Call { name, args } => self
                .call(name, args, e.span)?
                .expect("checker ensures the function returns a value"),
            ExprKind::Swizzle { base, mask } => {
                let v = self.eval(base)?;
                ops::swizzle(v, &mask.name)
            }
        };
        Ok(value)
    }

    /// Calls a user function or a built-in. `None` if it returns nothing.
    fn call(&mut self, name: &Ident, args: &'p [Expr], span: Span) -> Result<Option<Value>> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.eval(arg)?);
        }

        if let Some(&f) = self.functions.get(name.name.as_str()) {
            return self.call_function(f, values, span);
        }

        let builtin = kmpl_builtins::lookup(&name.name).expect("checker ensures it exists");
        (builtin.func)(&values, self.out).map_err(|m| error(m, span))
    }

    fn call_function(
        &mut self,
        f: &'p Function,
        args: Vec<Value>,
        span: Span,
    ) -> Result<Option<Value>> {
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(error(
                format!("stack overflow: more than {MAX_CALL_DEPTH} nested calls"),
                span,
            ));
        }

        let params = f
            .params
            .iter()
            .zip(args)
            .map(|(p, v)| (p.name.name.as_str(), Some(v)))
            .collect();
        self.frames.push(Frame {
            scopes: vec![params],
        });

        let flow = self.block(&f.body);
        self.frames.pop();

        match flow? {
            Flow::Return(value) => Ok(value),
            // Only functions without `-> type` can reach the end.
            _ => Ok(None),
        }
    }
}
