use kmpl_ast::{BinOp, Span, Ty, Type};

use crate::{
    Block, BlockId, Constant, FuncId, Function, Inst, Module, Op, Reg, Step, Target, Terminator,
    annotate,
};

const NO_SPAN: Span = Span { start: 0, end: 0 };

fn inst(dest: u32, op: Op) -> Inst {
    Inst {
        dest: Some(Reg(dest)),
        op,
        span: NO_SPAN,
        frees: Vec::new(),
    }
}

fn block(params: &[u32], insts: Vec<Inst>, term: Terminator) -> Block {
    Block {
        params: params.iter().map(|p| Reg(*p)).collect(),
        insts,
        term,
        term_frees: Vec::new(),
    }
}

fn jump(to: u32, args: &[u32]) -> Terminator {
    Terminator::Jump(Target {
        block: BlockId(to),
        args: args.iter().map(|a| Reg(*a)).collect(),
    })
}

fn module(params: &[u32], registers: usize, blocks: Vec<Block>) -> Module {
    Module {
        functions: vec![Function {
            name: "f".to_owned(),
            params: params.iter().map(|p| Reg(*p)).collect(),
            ret: None,
            blocks,
            types: vec![Ty::Prim(Type::Int); registers],
        }],
        init: FuncId(0),
        main: FuncId(0),
        globals: 0,
        builtins: Vec::new(),
        structs: Vec::new(),
    }
}

fn add(a: u32, b: u32) -> Op {
    Op::Binary {
        op: BinOp::Add,
        a: Reg(a),
        b: Reg(b),
    }
}

fn set_first(base: u32, value: u32) -> Op {
    Op::PathSet {
        base: Reg(base),
        path: vec![Step::Field(0)],
        value: Reg(value),
        take: false,
    }
}

#[test]
fn prints_the_text_form() {
    let m = module(
        &[0],
        4,
        vec![block(
            &[0],
            vec![
                inst(1, Op::Const(Constant::Real(1.0))),
                inst(2, add(0, 1)),
                inst(
                    3,
                    Op::Binary {
                        op: BinOp::Lt,
                        a: Reg(2),
                        b: Reg(1),
                    },
                ),
            ],
            Terminator::Return(Some(Reg(2))),
        )],
    );
    assert_eq!(
        m.to_string(),
        "fn f(v0: int) {\nb0:\n    v1 = const 1.0\n    v2 = add v0, v1\n    v3 = lt v2, v1\n    ret v2\n}\n"
    );
}

#[test]
fn prints_block_parameters_and_branches() {
    let m = module(
        &[0],
        2,
        vec![
            block(
                &[0],
                Vec::new(),
                Terminator::Branch {
                    cond: Reg(0),
                    then: Target {
                        block: BlockId(1),
                        args: vec![Reg(0)],
                    },
                    otherwise: Target {
                        block: BlockId(2),
                        args: Vec::new(),
                    },
                },
            ),
            block(&[1], Vec::new(), Terminator::Return(None)),
            block(&[], Vec::new(), Terminator::Unreachable),
        ],
    );
    let text = m.to_string();
    assert!(text.contains("br v0, b1(v0), b2"), "{text}");
    assert!(text.contains("b1(v1: int):"), "{text}");
    assert!(text.contains("unreachable"), "{text}");
}

#[test]
fn last_use_is_freed() {
    let mut m = module(
        &[0],
        3,
        vec![block(
            &[0],
            vec![inst(1, add(0, 0)), inst(2, add(1, 1))],
            Terminator::Return(Some(Reg(2))),
        )],
    );
    annotate(&mut m);
    let b = &m.functions[0].blocks[0];
    assert_eq!(b.insts[0].frees, vec![Reg(0)]);
    assert_eq!(b.insts[1].frees, vec![Reg(1)]);
    assert_eq!(b.term_frees, vec![Reg(2)]);
}

#[test]
fn unused_result_is_freed() {
    let mut m = module(
        &[0],
        2,
        vec![block(
            &[0],
            vec![inst(1, add(0, 0))],
            Terminator::Return(Some(Reg(0))),
        )],
    );
    annotate(&mut m);
    assert_eq!(m.functions[0].blocks[0].insts[0].frees, vec![Reg(1)]);
}

#[test]
fn value_used_in_a_later_block_is_kept() {
    let mut m = module(
        &[0],
        2,
        vec![
            block(&[0], vec![inst(1, add(0, 0))], jump(1, &[])),
            block(&[], Vec::new(), Terminator::Return(Some(Reg(0)))),
        ],
    );
    annotate(&mut m);
    let f = &m.functions[0];
    assert!(!f.blocks[0].insts[0].frees.contains(&Reg(0)));
    assert_eq!(f.blocks[1].term_frees, vec![Reg(0)]);
}

#[test]
fn value_used_around_a_loop_stays_live() {
    // b1 reads v0 every round, so its use there must not end it.
    let mut m = module(
        &[0],
        3,
        vec![
            block(&[0], Vec::new(), jump(1, &[0])),
            block(&[1], vec![inst(2, add(0, 1))], jump(1, &[2])),
        ],
    );
    annotate(&mut m);
    let f = &m.functions[0];
    assert!(!f.blocks[1].insts[0].frees.contains(&Reg(0)));
    assert!(f.blocks[1].insts[0].frees.contains(&Reg(1)));
}

#[test]
fn path_set_takes_a_dead_base_only() {
    let mut m = module(
        &[0, 1],
        3,
        vec![block(
            &[0, 1],
            vec![inst(2, set_first(0, 1))],
            Terminator::Return(Some(Reg(2))),
        )],
    );
    annotate(&mut m);
    assert!(matches!(
        m.functions[0].blocks[0].insts[0].op,
        Op::PathSet { take: true, .. }
    ));

    let mut m = module(
        &[0, 1],
        3,
        vec![block(
            &[0, 1],
            vec![inst(2, set_first(0, 1))],
            Terminator::Return(Some(Reg(0))),
        )],
    );
    annotate(&mut m);
    assert!(matches!(
        m.functions[0].blocks[0].insts[0].op,
        Op::PathSet { take: false, .. }
    ));
}

#[test]
fn a_register_is_freed_once_per_instruction() {
    let mut m = module(
        &[0],
        2,
        vec![block(
            &[0],
            vec![inst(1, add(0, 0))],
            Terminator::Return(Some(Reg(1))),
        )],
    );
    annotate(&mut m);
    assert_eq!(m.functions[0].blocks[0].insts[0].frees, vec![Reg(0)]);
}
