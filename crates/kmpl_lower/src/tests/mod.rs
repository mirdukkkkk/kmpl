use std::collections::HashSet;

use kmpl_ir::{Function, Module, Reg, Terminator};

use crate::lower;

fn lowered(src: &str) -> Module {
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    let types = kmpl_check::check(&mut program).unwrap();
    let mut module = lower(&program, &types);
    kmpl_ir::annotate(&mut module);
    module
}

fn optimized(src: &str) -> Module {
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    let types = kmpl_check::check(&mut program).unwrap();
    let mut module = lower(&program, &types);
    kmpl_ir::optimize(&mut module);
    module
}

/// The text of the function called `name`, after optimizing.
fn optimized_text(src: &str, name: &str) -> String {
    function_text(&optimized(src), name)
}

/// The text of the function called `name`.
fn text_of(src: &str, name: &str) -> String {
    function_text(&lowered(src), name)
}

fn function_text(module: &Module, name: &str) -> String {
    let text = module.to_string();
    let start = text
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("no `{name}` in\n{text}"));
    let end = text[start..].find("\n}\n").unwrap() + start + 3;
    text[start..end].to_owned()
}

/// Every register is defined exactly once and before the end of the
/// function, and every jump passes as many values as its target expects.
fn verify(f: &Function) {
    let mut defined = HashSet::new();
    let mut define = |reg: Reg| assert!(defined.insert(reg), "{} defined twice: {reg}", f.name);

    for block in &f.blocks {
        block.params.iter().copied().for_each(&mut define);
        for inst in &block.insts {
            inst.dest.into_iter().for_each(&mut define);
        }
    }

    let check_use = |reg: Reg| assert!(defined.contains(&reg), "{}: undefined {reg}", f.name);
    for block in &f.blocks {
        for inst in &block.insts {
            inst.op.for_each_use(check_use);
        }
        block.term.for_each_use(check_use);

        let targets = match &block.term {
            Terminator::Jump(target) => vec![target],
            Terminator::Branch {
                then, otherwise, ..
            } => vec![then, otherwise],
            Terminator::Return(_) | Terminator::Unreachable => Vec::new(),
        };
        for target in targets {
            let params = f.blocks[target.block.0 as usize].params.len();
            assert_eq!(target.args.len(), params, "{}: bad jump", f.name);
        }
    }
    assert_eq!(f.blocks[0].params, f.params);
}

#[test]
fn arithmetic_is_a_list_of_instructions() {
    let src = "fn f(x: real) -> real { return x * x + 1.0; } fn main() { }";
    assert_eq!(
        text_of(src, "f"),
        "fn f(v0: real) -> real {\n\
         b0:\n    \
         v1 = mul v0, v0\n    \
         v2 = const 1.0\n    \
         v3 = add v1, v2\n    \
         ret v3\n\
         }\n"
    );
}

#[test]
fn assignment_makes_a_new_register() {
    let src = "fn f(x: int) -> int { x += 1; x *= 2; return x; } fn main() { }";
    let text = text_of(src, "f");
    assert!(text.contains("v2 = add v0, v1"), "{text}");
    assert!(text.contains("v4 = mul v2, v3"), "{text}");
    assert!(text.contains("ret v4"), "{text}");
}

#[test]
fn branches_merge_through_a_parameter() {
    let src = "fn f(c: bool) -> int {
                   let x = 1;
                   if c { x = 2; } else { x = 3; }
                   return x;
               }
               fn main() { }";
    let text = text_of(src, "f");
    assert!(text.contains("(v4: int):"), "{text}");
    assert!(text.contains("ret v4"), "{text}");
}

#[test]
fn unchanged_variables_need_no_parameter() {
    let src = "fn f(c: bool) -> int {
                   let x = 1;
                   if c { print(1); }
                   return x;
               }
               fn main() { }";
    let text = text_of(src, "f");
    assert!(!text.contains("):"), "{text}");
    assert!(text.contains("ret v1"), "{text}");
}

#[test]
fn loops_carry_assigned_variables() {
    let src = "fn f(n: int) -> int {
                   let total = 0;
                   let unused = 5;
                   let i = 0;
                   while i < n { total += i; i += 1; }
                   return total + unused;
               }
               fn main() { }";
    let m = lowered(src);
    let f = &m.functions[0];
    // The header takes `total` and `i`, not `unused`.
    assert_eq!(f.blocks[1].params.len(), 2);
}

#[test]
fn a_returning_branch_does_not_reach_the_join() {
    let src = "fn f(c: bool) -> int {
                   let x;
                   if c { return 1; } else { x = 2; }
                   return x;
               }
               fn main() { }";
    let m = lowered(src);
    verify(&m.functions[0]);
}

#[test]
fn code_after_return_is_dropped() {
    let src = "fn f() -> int { return 1; print(2); return 3; } fn main() { }";
    let text = text_of(src, "f");
    assert_eq!(text.matches("ret").count(), 1, "{text}");
}

#[test]
fn array_writes_are_updates() {
    let src = "fn f(a: [int]) -> [int] { a[0] = 1; return a; } fn main() { }";
    let text = text_of(src, "f");
    assert!(text.contains("v3 = set v0[v2], v1"), "{text}");
    assert!(text.contains("ret v3"), "{text}");
}

#[test]
fn a_write_to_an_otherwise_unused_array_takes_it() {
    let src = "fn f(a: [int]) -> [int] { a[0] = 1; return a; } fn main() { }";
    let m = lowered(src);
    let set = m.functions[0].blocks[0]
        .insts
        .iter()
        .find(|i| matches!(i.op, kmpl_ir::Op::PathSet { .. }))
        .unwrap();
    assert!(matches!(set.op, kmpl_ir::Op::PathSet { take: true, .. }));
}

#[test]
fn a_write_to_a_copy_keeps_the_original() {
    let src = "fn f(a: [int]) -> int { let b = a; b[0] = 1; return a[0] + b[0]; } fn main() { }";
    let m = lowered(src);
    let set = m.functions[0].blocks[0]
        .insts
        .iter()
        .find(|i| matches!(i.op, kmpl_ir::Op::PathSet { .. }))
        .unwrap();
    assert!(matches!(set.op, kmpl_ir::Op::PathSet { take: false, .. }));
}

#[test]
fn constants_are_computed_by_init() {
    let src = "const A: int = 1; const B: int = A + 1; fn main() { print(B); }";
    let m = lowered(src);
    assert_eq!(m.globals, 2);
    let init = &m.functions[m.init.0 as usize];
    assert_eq!(init.name, "__init");
    let text = text_of(src, "__init");
    assert!(text.contains("set_global 0"), "{text}");
    assert!(text.contains("set_global 1"), "{text}");
}

#[test]
fn every_example_is_well_formed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "kmpl") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        if src.contains("\nimport ") || src.starts_with("import ") {
            continue;
        }
        for f in lowered(&src)
            .functions
            .iter()
            .chain(&optimized(&src).functions)
        {
            verify(f);
        }
        checked += 1;
    }
    assert!(checked > 10, "only {checked} examples");
}

#[test]
fn every_kind_of_statement_is_well_formed() {
    let src = r#"
        struct P { x: real, v: [int] }
        const K: P = P { x: 1.0, v: [1] };
        fn id<T: int | real>(x: T) -> T { return x; }
        fn f(c: bool, a: [int], p: P) -> (int, real) {
            let (n, _) = (1, 2);
            let u;
            if c { u = 1; } else if n > 0 { return (0, 0.0); } else { u = 2; }
            for i in 0..=3 step 1 {
                if i == 1 { continue; }
                if i == 2 { break; }
                a[i] += id(u);
            }
            for x in a {
                p.x += real(x);
                p.v[0] = x;
            }
            while c && n > 0 || !c {
                n -= 1;
                if n < -3 { break; }
            }
            print(a[1..] + a[..1]);
            print(vec2(1, 2):yx);
            return (n, p.x + K.x + id(1.5));
        }
        fn main() { }
    "#;
    for f in lowered(src)
        .functions
        .iter()
        .chain(&optimized(src).functions)
    {
        verify(f);
    }
}

#[test]
fn constants_are_folded() {
    let src = "fn f(x: real) -> real { return 2.0 * 3.0 + x; } fn main() { }";
    let text = optimized_text(src, "f");
    assert!(text.contains("const 6.0"), "{text}");
    assert!(!text.contains("mul"), "{text}");

    let src = "fn f() -> bool { return 2 + 3 == 5 && !false; } fn main() { }";
    let text = optimized_text(src, "f");
    assert!(!text.contains("add"), "{text}");
}

#[test]
fn folding_keeps_runtime_errors() {
    let overflow = "fn f() -> int { return 9223372036854775807 + 1; } fn main() { }";
    assert!(optimized_text(overflow, "f").contains("add"));
    let zero = "fn f() -> int { return 1 / 0; } fn main() { }";
    assert!(optimized_text(zero, "f").contains("div"));
    let neg = "fn f() -> int { return -(-9223372036854775807 - 1); } fn main() { }";
    assert!(optimized_text(neg, "f").contains("neg"));
}

#[test]
fn floats_are_not_rearranged() {
    let src = "fn f(x: real) -> real { return x + 1.0 + 2.0; } fn main() { }";
    let text = optimized_text(src, "f");
    assert_eq!(text.matches("add").count(), 2, "{text}");
}

#[test]
fn a_range_with_a_known_step_is_a_comparison() {
    let src = "fn f(n: int) { for i in 0..n { print(i); } } fn main() { }";
    let text = optimized_text(src, "f");
    assert!(text.contains("lt"), "{text}");
    assert!(!text.contains("in_range"), "{text}");
    let src = "fn f(n: int) { for i in n..=0 step -2 { print(i); } } fn main() { }";
    assert!(optimized_text(src, "f").contains("ge"));
}

#[test]
fn known_branches_disappear() {
    let src = "fn f() -> int { if 1 < 2 { return 1; } return 2; } fn main() { }";
    let text = optimized_text(src, "f");
    assert!(!text.contains("br "), "{text}");
    assert_eq!(text.matches("ret").count(), 1, "{text}");
}

#[test]
fn unchanged_parameters_disappear() {
    let src = "fn f(n: int) -> int {
                   let x = 1;
                   let i = 0;
                   while i < n { x = x; i += 1; }
                   return x;
               }
               fn main() { }";
    let m = optimized(src);
    let params: usize = m.functions[0]
        .blocks
        .iter()
        .skip(1)
        .map(|b| b.params.len())
        .sum();
    // Only `i` still changes around the loop.
    assert_eq!(params, 1, "{m}");
}

#[test]
fn straight_code_is_one_block() {
    let src = "fn f(c: bool) -> int { let x = 1; if true { x = 2; } return x; } fn main() { }";
    let m = optimized(src);
    assert_eq!(m.functions[0].blocks.len(), 1, "{m}");
}

#[test]
fn unused_values_are_removed() {
    let src = "fn f(x: real) -> real { let a = x * 1.5; return x; } fn main() { }";
    let text = optimized_text(src, "f");
    assert!(!text.contains("mul"), "{text}");
}

#[test]
fn a_value_that_only_feeds_a_loop_is_removed() {
    let src = "fn f(n: int) {
                   let x = 0.0;
                   let i = 0;
                   while i < n { x = x * 2.0; i += 1; }
               }
               fn main() { }";
    let text = optimized_text(src, "f");
    assert!(!text.contains("mul"), "{text}");
}

#[test]
fn what_can_fail_or_has_an_effect_stays() {
    let div = "fn f(x: int) { let a = 1 / x; } fn main() { }";
    assert!(optimized_text(div, "f").contains("div"));
    let add = "fn f(x: int) { let a = x + 1; } fn main() { }";
    assert!(optimized_text(add, "f").contains("add"));
    let neg = "fn f(x: int) { let a = -x; } fn main() { }";
    assert!(optimized_text(neg, "f").contains("neg"));
    let index = "fn f(a: [int]) { let x = a[5]; } fn main() { }";
    assert!(optimized_text(index, "f").contains("index"));
    let call = "fn g() -> int { return 1; } fn f() { let x = g(); } fn main() { }";
    assert!(optimized_text(call, "f").contains("call g"));
    let builtin = "fn f(x: real) { let a = sqrt(x); } fn main() { }";
    assert!(optimized_text(builtin, "f").contains("builtin sqrt"));
    let arrays = "fn f(a: [real], b: [real]) { let c = a + b; } fn main() { }";
    assert!(optimized_text(arrays, "f").contains("add"));
}

#[test]
fn registers_have_no_gaps_after_optimizing() {
    let src = "fn f(n: int) -> int {
                   let unused = n * 2 + 1;
                   let total = 0;
                   for i in 0..n { total += i; }
                   return total;
               }
               fn main() { }";
    for f in &optimized(src).functions {
        let mut defined: Vec<u32> = f
            .blocks
            .iter()
            .flat_map(|b| {
                b.params
                    .iter()
                    .map(|r| r.0)
                    .chain(b.insts.iter().filter_map(|i| i.dest.map(|d| d.0)))
            })
            .collect();
        defined.sort_unstable();
        let expected: Vec<u32> = (0..f.types.len() as u32).collect();
        assert_eq!(defined, expected, "{}", f.name);
    }
}

/// Index of the block a loop's header jump comes back to, with its text.
fn block_of(text: &str, needle: &str) -> usize {
    let at = text.find(needle).unwrap_or_else(|| panic!("no {needle} in\n{text}"));
    text[..at].matches("\nb").count()
}

#[test]
fn constants_leave_loops() {
    let src = "fn f(n: int) -> real {
                   let s = 0.0;
                   let i = 0;
                   while i < n { s += 0.5; i += 1; }
                   return s;
               }
               fn main() { }";
    let text = optimized_text(src, "f");
    assert_eq!(block_of(&text, "const 0.5"), 1, "{text}");
}

#[test]
fn work_on_unchanging_values_leaves_loops() {
    let src = "fn f(x: real, n: int) -> real {
                   let s = 0.0;
                   let i = 0;
                   while i < n { s += x * x; i += 1; }
                   return s;
               }
               fn main() { }";
    let text = optimized_text(src, "f");
    assert_eq!(block_of(&text, "mul"), 1, "{text}");
}

#[test]
fn work_leaves_nested_loops() {
    let src = "fn f(n: int) -> real {
                   let s = 0.0;
                   for i in 0..n {
                       for j in 0..n { s += 0.25; }
                   }
                   return s;
               }
               fn main() { }";
    let text = optimized_text(src, "f");
    assert_eq!(block_of(&text, "const 0.25"), 1, "{text}");
}

#[test]
fn failing_work_stays_in_the_loop() {
    let src = "fn f(a: [int], x: int, n: int) -> int {
                   let s = 0;
                   let i = 0;
                   while i < n { s += 10 / x; s += a[9]; i += 1; }
                   return s;
               }
               fn main() { }";
    let text = optimized_text(src, "f");
    assert!(block_of(&text, "div") > 1, "{text}");
    assert!(block_of(&text, "index") > 1, "{text}");
}

#[test]
fn work_that_depends_on_the_loop_stays() {
    let src = "fn f(n: int) -> real {
                   let s = 1.5;
                   let i = 0;
                   while i < n { s = s * s; i += 1; }
                   return s;
               }
               fn main() { }";
    let text = optimized_text(src, "f");
    assert!(block_of(&text, "mul") > 1, "{text}");
}

#[test]
fn a_loop_entered_from_a_branch_gets_a_preheader() {
    let src = "fn f(c: bool, n: int) -> real {
                   let s = 0.0;
                   if c {
                       let i = 0;
                       while i < n { s += 0.5; i += 1; }
                   }
                   return s;
               }
               fn main() { }";
    let m = optimized(src);
    verify(&m.functions[0]);
}

#[test]
fn arrays_are_not_moved_out_of_loops() {
    let src = "fn f(n: int) {
                   for i in 0..n { let a = [1, 2]; a[0] = i; print(a); }
               }
               fn main() { }";
    let text = optimized_text(src, "f");
    assert!(block_of(&text, "array [") > 1, "{text}");
}
