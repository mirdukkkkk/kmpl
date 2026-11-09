use kmpl_ast::{Ident, ImportNames, ImportedItem, Item, Span};

use crate::{Module, ModuleImport, check, link};

fn ident(name: &str) -> Ident {
    Ident {
        name: name.to_owned(),
        span: Span::new(0, 0),
    }
}

/// A module with `imports` as (name in the code, full name of the module).
fn module(prefix: &str, imports: &[(&str, &str)], src: &str) -> Module {
    Module {
        prefix: prefix.to_owned(),
        imports: imports
            .iter()
            .map(|(name, target)| ModuleImport {
                names: ImportNames::Module(ident(name)),
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
        Ok(_) => Vec::new(),
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
            Item::Struct(s) => s.name.name.as_str(),
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
            "`a` is not a variable or an imported module",
            "`b` is not a variable or an imported module"
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
        ["`c` is not a variable or an imported module"]
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
        errors(vec![
            module("a", &[], "const X: int = 1;"),
            module("", &[("a", "a")], "fn main() { let a = 1; print(a.X); }"),
        ]),
        ["only a struct has fields, found `int`, `real` or `complex`"]
    );
    assert_eq!(
        errors(vec![
            module("a", &[], "fn f() { }"),
            module("", &[("a", "a")], "fn main() { let a = 1; a.f(); }"),
        ]),
        ["`a` is a variable, not a module"]
    );
}

#[test]
fn structs_in_modules() {
    assert_ok(vec![
        module(
            "lib.g",
            &[],
            "struct P { x: real }
             struct L { a: P, b: [P] }
             const O: P = P { x: 0.0 };
             fn make(x: real) -> P { let p: P = P { x: x }; return p; }",
        ),
        module(
            "",
            &[("g", "lib.g")],
            "fn f(p: g.P) -> [g.P] { return [p, g.make(1.0), g.O]; }
             fn main() {
                 let p: g.P = g.P { x: 1.0 };
                 let l = g.L { a: p, b: f(p) };
                 print(l.a.x + g.O.x + g.make(2.0).x);
                 l.b[0].x = 3.0;
             }",
        ),
    ]);
}

#[test]
fn local_does_not_hide_a_type() {
    assert_ok(vec![
        module("g", &[], "struct P { x: real }"),
        module(
            "",
            &[("g", "g")],
            "fn main() { let g = 1; let a: [g.P] = []; print(len(a)); }",
        ),
    ]);
}

#[test]
fn local_hides_own_struct_constant_but_not_the_type() {
    assert_ok(vec![
        module(
            "a",
            &[],
            "struct P { x: real }
             fn f(P: int) -> P { let y: int = P; return P { x: 1.0 }; }",
        ),
        module("", &[("a", "a")], MAIN),
    ]);
}

#[test]
fn generic_parameter_hides_a_struct() {
    assert_ok(vec![
        module(
            "a",
            &[],
            "struct T { x: real }
             fn f<T: int>(v: T) -> T { return v; }",
        ),
        module("", &[("a", "a")], MAIN),
    ]);
}

#[test]
fn missing_types_in_modules() {
    assert_eq!(
        link_errors(vec![
            module("g", &[], ""),
            module("", &[("g", "g")], "fn f(p: g.P) { } fn main() { }"),
        ]),
        ["module `g` has no `P`"]
    );
    assert_eq!(
        link_errors(vec![module("", &[], "fn f(p: g.P) { } fn main() { }")]),
        ["unknown module `g`, add `import g from \"...\";`"]
    );
    assert_eq!(
        link_errors(vec![
            module("g", &[], "struct P { }"),
            module("", &[("g", "g")], "fn main() { let g = 1; g.P { }; }",),
        ]),
        ["`g` is a variable, not a module"]
    );
}

#[test]
fn modules_cannot_reuse_built_in_type_names() {
    assert_eq!(
        link_errors(vec![
            module("a", &[], "struct bool { }"),
            module("", &[("a", "a")], MAIN),
        ]),
        ["`bool` is a built-in type"]
    );
}

#[test]
fn fields_of_module_constants_and_own_constants() {
    assert_ok(vec![
        module(
            "a",
            &[],
            "struct P { x: real }
             const O: P = P { x: 1.0 };
             fn f() -> real { return O.x; }",
        ),
        module("", &[("a", "a")], "fn main() { print(a.O.x); }"),
    ]);
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

#[test]
fn a_call_or_type_from_an_unknown_module_suggests_an_import() {
    assert_eq!(
        link_errors(vec![module("", &[], "fn main() { a.f(); }")]),
        ["unknown module `a`, add `import a from \"...\";`"]
    );
}

/// A module importing `items` (name, alias) from `target`.
fn importing_items(target: &str, items: &[(&str, &str)], src: &str) -> Module {
    let mut importer = module("", &[], src);
    importer.imports.push(ModuleImport {
        names: ImportNames::Items(
            items
                .iter()
                .map(|(name, alias)| ImportedItem {
                    name: ident(name),
                    alias: ident(alias),
                })
                .collect(),
        ),
        target: target.to_owned(),
    });
    importer
}

const LIB: &str = "const K: int = 2; struct P { x: int } fn twice(n: int) -> int { return n * K; }";

#[test]
fn imported_items_are_used_without_a_prefix() {
    assert_ok(vec![
        module("lib", &[], LIB),
        importing_items(
            "lib",
            &[("twice", "twice"), ("K", "K"), ("P", "P")],
            "fn main() { let p: P = P { x: K }; print(twice(p.x)); }",
        ),
    ]);
}

#[test]
fn an_alias_replaces_the_name() {
    assert_ok(vec![
        module("lib", &[], LIB),
        importing_items(
            "lib",
            &[("twice", "double")],
            "fn main() { print(double(1)); }",
        ),
    ]);
}

#[test]
fn a_local_variable_hides_an_imported_constant() {
    assert_ok(vec![
        module("lib", &[], LIB),
        importing_items(
            "lib",
            &[("K", "K")],
            "fn main() { let K = 1.5; print(K * 2.0); }",
        ),
    ]);
}

#[test]
fn imported_items_must_exist() {
    assert_eq!(
        link_errors(vec![
            module("lib", &[], LIB),
            importing_items("lib", &[("nope", "nope")], MAIN),
        ]),
        ["this module has no `nope`"]
    );
}

#[test]
fn imported_items_cannot_clash() {
    for (src, items, message) in [
        (
            "fn twice() { } fn main() { }",
            &[("twice", "twice")][..],
            "`twice` is already defined in this file",
        ),
        (
            MAIN,
            &[("twice", "len")][..],
            "`len` is a built-in function",
        ),
        (MAIN, &[("twice", "PI")][..], "`PI` is a built-in constant"),
        (
            MAIN,
            &[("twice", "real")][..],
            "`real` is a built-in function",
        ),
        (
            MAIN,
            &[("twice", "a"), ("K", "a")][..],
            "`a` is already imported",
        ),
    ] {
        assert_eq!(
            link_errors(vec![
                module("lib", &[], LIB),
                importing_items("lib", items, src)
            ]),
            [message]
        );
    }
}
