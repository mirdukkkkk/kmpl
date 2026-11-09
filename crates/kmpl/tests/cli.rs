//! Runs the real `kmpl` binary and checks exit codes and output.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Writes `src` to a fresh temp file named after the test.
fn source_file(test: &str, src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kmpl-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{test}.kmpl"));
    std::fs::write(&path, src).unwrap();
    path
}

fn kmpl(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kmpl"))
        .args(args)
        .output()
        .unwrap()
}

fn run(test: &str, src: &str) -> Output {
    let path = source_file(test, src);
    kmpl(&[path.as_os_str()])
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn no_arguments_prints_usage() {
    let out = kmpl(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains(
        "usage: kmpl [--ast | --ir | --llvm-ir] [-v | -vv] [--no-opt] [--locked] <file>"
    ));
}

#[test]
fn missing_file() {
    let out = kmpl(&["/nonexistent/x.kmpl".as_ref()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).starts_with("/nonexistent/x.kmpl: "));
}

/// The `-->` location line of every error in `stderr`.
fn locations(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("--> "))
        .map(|loc| loc.rsplit('/').next().unwrap().to_owned())
        .collect()
}

#[test]
fn lex_error() {
    let out = run("lex_error", "fn main() { let x = 1 $ 2; }");
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.starts_with("error: unexpected character `$`\n"),
        "{err}"
    );
    assert_eq!(locations(&err), vec!["lex_error.kmpl:1:23"]);
    assert!(
        err.ends_with("1 | fn main() { let x = 1 $ 2; }\n  |                       ^\n"),
        "{err}"
    );
}

#[test]
fn too_large_integer() {
    let out = run("big_int", "fn main() { print(99999999999999999999); }");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out)
            .starts_with("error: integer literal `99999999999999999999` is too large for `int`\n")
    );
}

#[test]
fn parse_error() {
    let out = run("parse_error", "fn main() {\n    let x = 1\n}\n");
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.starts_with("error: expected `;`, found `}`\n"), "{err}");
    assert_eq!(locations(&err), vec!["parse_error.kmpl:3:1"]);
    assert!(err.ends_with("3 | }\n  | ^\n"), "{err}");
}

#[test]
fn check_errors_are_all_printed() {
    let out = run("check_errors", "fn main() { a = 1; b(); }");
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("error: undefined variable `a`\n"), "{err}");
    assert!(err.contains("error: undefined function `b`\n"), "{err}");
    assert_eq!(
        locations(&err),
        vec!["check_errors.kmpl:1:13", "check_errors.kmpl:1:20"]
    );
    // Errors are separated by a blank line.
    assert!(err.contains("^\n\nerror: "), "{err}");
}

#[test]
fn unknown_flag_prints_usage() {
    let out = kmpl(&["--tokens".as_ref(), "x.kmpl".as_ref()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("usage:"));
}

#[test]
fn runs_the_program() {
    let out = run("runs", "fn main() { print(1 + 2); print(sqrt(4.0)); }");
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "3\n2.0\n");
    assert!(stderr(&out).is_empty());
}

#[test]
fn ast_flag_prints_ast() {
    let path = source_file("ast", "fn main() { print(1); }");
    let out = kmpl(&["--ast".as_ref(), path.as_os_str()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).starts_with("Program {"));
}

#[test]
fn runtime_error_after_output() {
    let out = run(
        "runtime_error",
        "fn main() { let z = 0; print(1); print(1 / z); }",
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "1\n");
    let err = stderr(&out);
    assert!(err.starts_with("error: division by zero\n"), "{err}");
    assert_eq!(locations(&err), vec!["runtime_error.kmpl:1:40"]);
    assert!(
        err.ends_with("|                                        ^^^^^\n"),
        "{err}"
    );
}

/// Writes `files` (relative path, source) to a fresh directory named after
/// the test and runs the first one.
fn run_project(test: &str, files: &[(&str, &str)]) -> Output {
    let dir = std::env::temp_dir().join(format!("kmpl-cli-{}-{test}", std::process::id()));
    for (name, src) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, src).unwrap();
    }
    let out = kmpl(&[dir.join(files[0].0).as_os_str()]);
    std::fs::remove_dir_all(&dir).unwrap();
    out
}

#[test]
fn imports_run_in_dependency_order() {
    let out = run_project(
        "imports_run",
        &[
            (
                "main.kmpl",
                "import a from \"./lib/a\"; const H: real = a.G * 2.0; fn main() { print(a.f()); print(H); }",
            ),
            (
                "lib/a.kmpl",
                "import b from \"./b\"; const G: real = b.B + 1.0; fn f() -> real { return G; }",
            ),
            ("lib/b.kmpl", "const B: real = 1.0;"),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "2.0\n4.0\n");
}

#[test]
fn a_shared_import_is_loaded_once() {
    let out = run_project(
        "shared_import",
        &[
            (
                "main.kmpl",
                "import a from \"./a\"; import b from \"./b\"; fn main() { print(a.f() + b.g()); }",
            ),
            (
                "a.kmpl",
                "import c from \"./c\"; fn f() -> int { return c.X; }",
            ),
            (
                "b.kmpl",
                "import c from \"./c\"; fn g() -> int { return c.X; }",
            ),
            ("c.kmpl", "const X: int = 21;"),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "42\n");
}

#[test]
fn check_errors_name_the_imported_file() {
    let out = run_project(
        "check_error_file",
        &[
            ("main.kmpl", "import a from \"./a\";\nfn main() { }\n"),
            ("a.kmpl", "\nfn f() -> int { return true; }\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("a.kmpl:2:24"), "{err}");
    assert!(err.contains("2 | fn f() -> int { return true; }"), "{err}");
}

#[test]
fn runtime_errors_name_the_imported_file() {
    let out = run_project(
        "runtime_error_file",
        &[
            (
                "main.kmpl",
                "import a from \"./a\";\nfn main() { print(a.f(0)); }\n",
            ),
            ("a.kmpl", "fn f(x: int) -> int {\n    return 1 / x;\n}\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("division by zero"), "{err}");
    assert!(err.contains("a.kmpl:2:12"), "{err}");
}

#[test]
fn parse_and_lex_errors_in_imports() {
    let out = run_project(
        "parse_error_import",
        &[
            ("main.kmpl", "import a from \"./a\";\nfn main() { }\n"),
            ("a.kmpl", "\nfn (\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("a.kmpl:2:4"), "{}", stderr(&out));

    let out = run_project(
        "lex_error_import",
        &[
            ("main.kmpl", "import a from \"./a\";\nfn main() { }\n"),
            ("a.kmpl", "\n$\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("unexpected character `$`"), "{err}");
    assert!(err.contains("a.kmpl:2:1"), "{err}");
}

#[test]
fn missing_module() {
    let out = run_project(
        "missing_module",
        &[(
            "main.kmpl",
            "import nope from \"./lib/nope\";\nfn main() { }\n",
        )],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("cannot find module `./lib/nope`"), "{err}");
    assert!(err.contains("main.kmpl:1:1"), "{err}");
}

#[test]
fn import_cycles() {
    let out = run_project(
        "import_cycle",
        &[
            ("main.kmpl", "import a from \"./a\";\nfn main() { }\n"),
            ("a.kmpl", "import b from \"./b\";\n"),
            ("b.kmpl", "import a from \"./a\";\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("import cycle: a -> b -> a"), "{err}");
    assert!(err.contains("b.kmpl:1:1"), "{err}");

    let out = run_project(
        "import_self_cycle",
        &[
            ("main.kmpl", "import a from \"./a\";\nfn main() { }\n"),
            ("a.kmpl", "import main from \"./main\";\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("import cycle: main -> a -> main"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn link_errors_are_reported_with_the_file() {
    let out = run_project(
        "link_error_file",
        &[("main.kmpl", "fn main() { print(a.x); }\n")],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("`a` is not a variable or an imported module"),
        "{err}"
    );
    assert!(err.contains("main.kmpl:1:19"), "{err}");
}

/// Runs the binary with `input` on stdin.
fn kmpl_with_input(args: &[&std::ffi::OsStr], input: &[u8]) -> Output {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_kmpl"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn help_is_printed_to_stdout() {
    for flags in [vec!["--help"], vec!["-h"], vec!["--ast", "--help"]] {
        let args: Vec<&std::ffi::OsStr> = flags.iter().map(|f| f.as_ref()).collect();
        let out = kmpl(&args);
        assert_eq!(out.status.code(), Some(0), "{flags:?}");
        let text = stdout(&out);
        assert!(
            text.contains(
                "kmpl [--ast | --ir | --llvm-ir] [-v | -vv] [--no-opt] [--locked] <file> [args...]"
            ),
            "{text}"
        );
        assert!(text.contains("--ast"), "{text}");
        assert!(text.contains("-h, --help"), "{text}");
        assert_eq!(stderr(&out), "");
    }
}

#[test]
fn arguments_after_the_file_go_to_the_program() {
    let path = source_file(
        "program_args",
        "import (args) from \"std:io\"; fn main() { print(args()); }",
    );
    let out = kmpl(&[
        path.as_os_str(),
        "one".as_ref(),
        "--help".as_ref(),
        "--ast".as_ref(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "[one, --help, --ast]\n");
}

#[test]
fn stdin_reaches_the_program() {
    let path = source_file(
        "program_stdin",
        r#"import (read_line) from "std:io"; import (parse_int) from "std:text";
        fn main() {
            print("number?");
            match read_line() {
                some(line) => {
                    match parse_int(line) {
                        some(n) => { print(n * 2); }
                        none => { panic("not a number"); }
                    }
                }
                none => { }
            }
        }"#,
    );
    let out = kmpl_with_input(&[path.as_os_str()], b"21\n");
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "number?\n42\n");
}

#[test]
fn end_of_input_without_any_data() {
    let path = source_file(
        "program_empty_stdin",
        "import (read_line) from \"std:io\"; fn main() { print(read_line()); }",
    );
    let out = kmpl_with_input(&[path.as_os_str()], b"");
    assert_eq!(stdout(&out), "none\n");
}

#[test]
fn ir_flag_prints_the_ir() {
    let path = source_file(
        "ir_flag",
        "fn f(x: real) -> real { return x * x; } fn main() { print(f(2.0)); }",
    );
    let out = kmpl(&["--ir".as_ref(), path.as_os_str()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("fn f(v0: real) -> real {"), "{text}");
    assert!(text.contains("v1 = mul v0, v0"), "{text}");
    assert!(!text.contains("4.0\n"), "must not run: {text}");
}

#[test]
fn no_opt_runs_the_program_the_same() {
    let path = source_file(
        "no_opt",
        "import (args) from \"std:io\"; fn main() { let x = 0; for i in 1..=3 { x += i; } print(x); print(args()); }",
    );
    let out = kmpl(&["--no-opt".as_ref(), path.as_os_str(), "a".as_ref()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "6\n[a]\n");
}

fn assert_too_deep(test: &str, src: &str) {
    let out = run(test, src);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("nested too deeply"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn deep_nesting_in_source_is_an_error_not_a_crash() {
    let n = 100_000;
    let open_close = |open: &str, close: &str| (open.repeat(n), close.repeat(n));

    let (open, close) = open_close("(", ")");
    assert_too_deep(
        "deep_parens",
        &format!("fn main() {{ print({open}1{close}); }}"),
    );

    let (open, close) = open_close("[", "]");
    assert_too_deep(
        "deep_arrays",
        &format!("fn main() {{ let a = {open}1{close}; }}"),
    );
    assert_too_deep(
        "deep_types",
        &format!("fn f(a: {open}int{close}) {{ }} fn main() {{ }}"),
    );

    let minus = "-".repeat(n);
    assert_too_deep("deep_unary", &format!("fn main() {{ print({minus}1); }}"));

    let (open, close) = open_close("if true { ", "} ");
    assert_too_deep("deep_blocks", &format!("fn main() {{ {open} {close} }}"));

    let chain = "if false { } else ".repeat(n);
    assert_too_deep("deep_else", &format!("fn main() {{ {chain} {{ }} }}"));
}

#[test]
fn reasonable_nesting_works() {
    let open = "(".repeat(200);
    let close = ")".repeat(200);
    let out = run(
        "ok_parens",
        &format!("fn main() {{ print({open}1{close}); }}"),
    );
    assert_eq!(stdout(&out), "1\n", "{}", stderr(&out));
}

#[test]
fn a_long_expression_compiles() {
    let chain = " + 1".repeat(20_000);
    let out = run("long_chain", &format!("fn main() {{ print(0{chain}); }}"));
    assert_eq!(stdout(&out), "20000\n", "{}", stderr(&out));
}

#[test]
fn deeply_nested_data_is_freed_printed_and_compared() {
    let build = "struct Node { children: [Node] }
                 fn deep(n: int) -> Node {
                     let node = Node { children: [] };
                     for i in 0..n { node = Node { children: [node] }; }
                     return node;
                 }";
    let out = run(
        "deep_data",
        &format!(
            "{build} fn main() {{
                 let a = deep(100000);
                 print(a == deep(100000));
                 print(length(string(a)) > 100000);
             }}"
        ),
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "true\ntrue\n");
}

#[test]
fn a_directory_is_found_through_mod_kmpl() {
    let out = run_project(
        "mod_kmpl",
        &[
            (
                "main.kmpl",
                "import a from \"./lib/a\";\nfn main() { print(a.f()); }\n",
            ),
            (
                "lib/a/mod.kmpl",
                "import b from \"./b\";\nfn f() -> int { return b.N; }\n",
            ),
            ("lib/a/b.kmpl", "const N: int = 7;\n"),
        ],
    );
    assert_eq!(stdout(&out), "7\n", "{}", stderr(&out));
}

#[test]
fn a_file_and_a_directory_of_one_name_are_ambiguous() {
    let out = run_project(
        "ambiguous_module",
        &[
            ("main.kmpl", "import a from \"./a\";\nfn main() { }\n"),
            ("a.kmpl", ""),
            ("a/mod.kmpl", ""),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("module `./a` is ambiguous"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn an_explicit_extension_names_exactly_that_file() {
    let out = run_project(
        "explicit_file",
        &[
            (
                "main.kmpl",
                "import a from \"./a.kmpl\";\nfn main() { print(a.N); }\n",
            ),
            ("a.kmpl", "const N: int = 1;\n"),
            ("a/mod.kmpl", "const N: int = 2;\n"),
        ],
    );
    assert_eq!(stdout(&out), "1\n", "{}", stderr(&out));
}

#[test]
fn a_module_outside_the_program_directory() {
    let out = run_project(
        "outside",
        &[
            (
                "app/main.kmpl",
                "import shared from \"../shared\";\nimport here from \"./shared\";\n\
                 fn main() { print(shared.N + here.N); }\n",
            ),
            ("shared.kmpl", "const N: int = 1;\n"),
            ("app/shared.kmpl", "const N: int = 20;\n"),
        ],
    );
    assert_eq!(stdout(&out), "21\n", "{}", stderr(&out));
}

#[test]
fn items_imported_by_name() {
    let out = run_project(
        "import_items",
        &[
            (
                "main.kmpl",
                "import (twice, K as FACTOR, Point) from \"./lib\";\n\
                 fn main() { let p = Point { x: FACTOR }; print(twice(p.x)); print(p); }\n",
            ),
            (
                "lib.kmpl",
                "const K: int = 4;\nstruct Point { x: int }\n\
                 fn twice(n: int) -> int { return n * 2; }\n",
            ),
        ],
    );
    assert_eq!(stdout(&out), "8\nPoint { x: 4 }\n", "{}", stderr(&out));
}

#[test]
fn a_module_path_must_be_relative() {
    let out = run_project(
        "bad_source",
        &[("main.kmpl", "import a from \"/etc/a\";\nfn main() { }\n")],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("`/etc/a` is not a module path"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn files_through_std_fs() {
    let dir = std::env::temp_dir().join(format!("kmpl-cli-{}-std-fs", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("note.bin");
    let src = format!(
        "import fs from \"std:fs\";
         fn main() {{
             let path = \"{}\";
             print(fs.write_text(path, \"héllo\") == \"\");
             let (text, error) = fs.read_text(path);
             print(text); print(error == \"\");
             let (data, _) = fs.read(path);
             print(length(data));
             data[0] = 255;
             print(fs.write(path, data) == \"\");
             let (broken, why) = fs.read_text(path);
             print(broken == \"\"); print(why != \"\");
             let (_, missing) = fs.read(path + \".nope\");
             print(missing != \"\");
         }}",
        file.display()
    );
    let out = run("std_fs", &src);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        stdout(&out),
        "true\nhéllo\ntrue\n6\ntrue\ntrue\ntrue\ntrue\n",
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_dot_call_is_a_call_with_the_value_first() {
    let out = run(
        "dot_calls",
        "fn scaled(v: vec2, k: real) -> vec2 { return v * k; }
         fn add(a: int, b: int) -> int { return a + b; }
         struct P { x: real, items: [int] }
         const C: int = 5;
         fn make() -> vec2 { return vec2(3.0, 4.0); }
         fn main() {
             let v = vec2(3.0, 4.0);
             print(v.length());
             print(v.scaled(2.0).length());
             print(make().length());
             let two = 2;
             print(two.add(3).add(4));
             let p = P { x: 4.0, items: [1, 2] };
             print(p.x.sqrt());
             print(p.items.length());
             print(p.items[0].add(1));
             print(C.add(1));
             print((1.0 + 3.0).sqrt());
             print(v.scaled(2.0):yx);
         }",
    );
    assert_eq!(
        stdout(&out),
        "5.0\n10.0\n5.0\n9\n2.0\n2\n2\n6\n2.0\n(8.0, 6.0)\n",
        "{}",
        stderr(&out)
    );
}

#[test]
fn optionals_read_well_with_a_dot() {
    let out = run(
        "dot_unwrap",
        "import (parse_int) from \"std:text\";
         fn main() {
             print(parse_int(\"4\").unwrap());
             print(parse_int(\"x\").unwrap_or(7));
             let a: real? = none;
             print(a.unwrap_or(2.5).sqrt() > 1.0);
         }",
    );
    assert_eq!(stdout(&out), "4\n7\ntrue\n", "{}", stderr(&out));
}

#[test]
fn every_std_library_can_be_imported() {
    let out = run(
        "std_all",
        "import (parse_int) from \"std:text\";
         import bits from \"std:bits\";
         import io from \"std:io\";
         import fs from \"std:fs\";
         fn main() { print(parse_int(\"4\")); print(bits.shr_logical(-1, 60)); print(length(io.args())); }",
    );
    assert_eq!(stdout(&out), "some(4)\n15\n0\n", "{}", stderr(&out));
}

#[test]
fn an_unknown_library_lists_the_known_ones() {
    let out = run("std_unknown", "import x from \"std:nope\"; fn main() { }");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("there is no library `std:nope`, there are: bits, fs, io, text"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn internal_functions_are_for_the_standard_library_only() {
    let out = run("std_internal", "fn main() { print(_io_args()); }");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("`_io_args` belongs to the standard library"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn moved_functions_need_an_import() {
    let out = run("moved", "fn main() { print(args()); }");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("args"), "{}", stderr(&out));
}

#[test]
fn a_plain_name_comes_from_kmpl_toml() {
    let out = run_project(
        "manifest",
        &[
            (
                "app/src/main.kmpl",
                "import helpers from \"helpers\";\nfn main() { print(helpers.N); }\n",
            ),
            (
                "app/kmpl.toml",
                "version = 1\n\n[dependencies]\nhelpers = \"./vendor/helpers\"\n",
            ),
            ("app/vendor/helpers/mod.kmpl", "const N: int = 5;\n"),
        ],
    );
    assert_eq!(stdout(&out), "5\n", "{}", stderr(&out));
}

#[test]
fn a_plain_name_needs_to_be_listed() {
    let out = run_project(
        "manifest_missing",
        &[
            ("main.kmpl", "import a from \"a\";\nfn main() { }\n"),
            ("kmpl.toml", "version = 1\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("`a` is not listed in"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_bad_kmpl_toml_is_reported_with_its_line() {
    let out = run_project(
        "manifest_bad",
        &[
            ("main.kmpl", "import a from \"a\";\nfn main() { }\n"),
            ("kmpl.toml", "[dependencies]\na = \"./a\"\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("`version = 1` is required"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_constant_can_use_a_pure_library() {
    let out = run(
        "const_std_pure",
        "import (parse_int) from \"std:text\"; const X: int? = parse_int(\"41\"); fn main() { print(X); }",
    );
    assert_eq!(stdout(&out), "some(41)\n", "{}", stderr(&out));
}

#[test]
fn a_constant_cannot_use_input() {
    let out = run(
        "const_std_io",
        "import (args) from \"std:io\"; const X: [string] = args(); fn main() { }",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("its value depends on how the program is run"),
        "{}",
        stderr(&out)
    );
}
