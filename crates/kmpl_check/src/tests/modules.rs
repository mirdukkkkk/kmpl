use kmpl_ast::{Ident, Item, Span};

use crate::{Module, ModuleImport, check, link};

/// A module with `imports` as (name in the code, full name of the module).
fn module(prefix: &str, imports: &[(&str, &str)], src: &str) -> Module {
    Module {
        prefix: prefix.to_owned(),
        imports: imports
            .iter()
            .map(|(name, target)| ModuleImport {
                name: Ident {
                    name: (*name).to_owned(),
                    span: Span::new(0, 0),
                },
                target: (*target).to_owned(),
            })
            .collect(),
        program: kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap(),
    }
}

fn link_errors(modules: Vec<Module>) -> Vec<String> {
    match link(modules) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
    }
}

/// Error messages of linking and then checking `modules`.
fn errors(modules: Vec<Module>) -> Vec<String> {
    let mut program = match link(modules) {
        Ok(program) => program,
        Err(errors) => return errors.into_iter().map(|e| e.message).collect(),
    };
    match check(&mut program) {
        Ok(()) => Vec::new(),
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
    }
}

fn assert_ok(modules: Vec<Module>) {
    assert_eq!(errors(modules), Vec::<String>::new());
}

const MAIN: &str = "fn main() { }";

#[test]
fn items_get_the_module_prefix() {
    let program = link(vec![
        module(
            "lib.a",
            &[],
            "const G: real = 1.0; fn f() -> real { return G; }",
        ),
        module("", &[("a", "lib.a")], MAIN),
    ])
    .unwrap();

    let names: Vec<&str> = program
        .items
        .iter()
        .map(|item| match item {
            Item::Const(c) => c.name.name.as_str(),
            Item::Function(f) => f.name.name.as_str(),
        })
        .collect();
    assert_eq!(names, ["lib.a.G", "lib.a.f", "main"]);
}

#[test]
fn names_inside_a_module_refer_to_it() {
    assert_ok(vec![
        module(
            "a",
            &[],
            "const G: real = 1.0;
             fn f() -> real { return g() + G; }
             fn g() -> real { return G; }",
        ),
        module("", &[("a", "a")], "fn main() { print(a.f()); print(a.G); }"),
    ]);
}

#[test]
fn imported_names_use_the_full_module_name() {
    assert_ok(vec![
        module("x.stats", &[], "fn mean() -> real { return 1.0; }"),
        module("y.stats", &[], "fn other() -> int { return 1; }"),
        module(
            "b",
            &[("stats", "y.stats")],
            "fn f() -> int { return stats.other(); }",
        ),
        module(
            "",
            &[("stats", "x.stats"), ("b", "b")],
            "fn main() { let r: real = stats.mean(); let i: int = b.f(); }",
        ),
    ]);
}

#[test]
fn locals_hide_items_of_the_module() {
    let with_body = |body: &str| {
        vec![
            module("a", &[], &format!("const G: real = 1.0; {body}")),
            module("", &[("a", "a")], MAIN),
        ]
    };
    assert_ok(with_body("fn f(G: int) -> int { return G; }"));
    assert_ok(with_body("fn f() { let G = 1; let y: int = G; }"));
    assert_ok(with_body("fn f() { let G = G; let y: real = G; }"));
    assert_ok(with_body("fn f() { for G in 0..1 { let y: int = G; } }"));
    assert_ok(with_body(
        "fn f(a: [int]) { for G in a { let y: int = G; } }",
    ));
    assert_ok(with_body("fn f() { let (G, _) = (1, 2); let y: int = G; }"));
    // The scope ends with its block.
    assert_ok(with_body(
        "fn f() { if true { let G = 1; } let y: real = G; }",
    ));
}

#[test]
fn constants_can_use_imported_constants_but_not_functions() {
    assert_ok(vec![
        module("a", &[], "const G: real = 2.0;"),
        module(
            "",
            &[("a", "a")],
            "const H: real = a.G * 2.0; fn main() { }",
        ),
    ]);
    assert_eq!(
        errors(vec![
            module("a", &[], "fn f() -> real { return 1.0; }"),
            module("", &[("a", "a")], "const H: real = a.f(); fn main() { }"),
        ]),
        ["a constant can only call built-in functions, `a.f` is not one"]
    );
}

#[test]
fn modules_have_generics() {
    assert_ok(vec![
        module(
            "a",
            &[],
            "fn twice<T: int | real>(x: T) -> T { return helper(x) + x; }
             fn helper<T: int | real>(x: T) -> T { return x; }",
        ),
        module(
            "",
            &[("a", "a")],
            "fn main() { let x: real = a.twice(1.5); }",
        ),
    ]);
}

#[test]
fn every_error_is_reported() {
    assert_eq!(
        link_errors(vec![module("", &[], "fn main() { a.x; b.y; }")]),
        [
            "unknown module `a`, add `import a;`",
            "unknown module `b`, add `import b;`"
        ]
    );
}

#[test]
fn imports_are_not_transitive() {
    assert_eq!(
        link_errors(vec![
            module("c", &[], "const X: int = 1;"),
            module("b", &[("c", "c")], ""),
            module("", &[("b", "b")], "fn main() { print(c.X); }"),
        ]),
        ["unknown module `c`, add `import c;`"]
    );
}

#[test]
fn missing_members() {
    assert_eq!(
        link_errors(vec![
            module("a", &[], "const X: int = 1;"),
            module("", &[("a", "a")], "fn main() { a.y; a.f(); }"),
        ]),
        ["module `a` has no `y`", "module `a` has no `f`"]
    );
}

#[test]
fn a_local_hides_a_module() {
    assert_eq!(
        link_errors(vec![
            module("a", &[], "const X: int = 1;"),
            module("", &[("a", "a")], "fn main() { let a = 1; print(a.X); }"),
        ]),
        ["`a` is a variable, not a module"]
    );
}

#[test]
fn two_imports_with_one_name() {
    assert_eq!(
        link_errors(vec![
            module("x.s", &[], ""),
            module("y.s", &[], ""),
            module("", &[("s", "x.s"), ("s", "y.s")], MAIN),
        ]),
        ["`s` is already imported"]
    );
}

#[test]
fn modules_cannot_reuse_built_in_names() {
    assert_eq!(
        link_errors(vec![
            module(
                "a",
                &[],
                "fn sqrt(x: real) -> real { return x; } const PI: real = 3.0;"
            ),
            module("", &[("a", "a")], MAIN),
        ]),
        [
            "`sqrt` is a built-in function",
            "`PI` is a built-in constant"
        ]
    );
}

#[test]
fn only_the_main_file_runs_main() {
    assert_ok(vec![
        module("a", &[], "fn main() { print(1); }"),
        module("", &[("a", "a")], MAIN),
    ]);
    assert_eq!(
        errors(vec![
            module("a", &[], "fn main() { }"),
            module("", &[("a", "a")], ""),
        ]),
        ["no `main` function in the program"]
    );
}

#[test]
fn module_items_cannot_be_assigned() {
    assert_eq!(
        errors(vec![
            module("a", &[], "const X: [int] = [1];"),
            module("", &[("a", "a")], "fn main() { a.X[0] = 2; }"),
        ]),
        ["cannot assign to constant `a.X`"]
    );
}
