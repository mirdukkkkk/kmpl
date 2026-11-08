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
    assert!(stderr(&out).contains("usage: kmpl [--ast] <file>"));
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
                "import lib.a; const H: real = a.G * 2.0; fn main() { print(a.f()); print(H); }",
            ),
            (
                "lib/a.kmpl",
                "import b; const G: real = b.B + 1.0; fn f() -> real { return G; }",
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
                "import a; import b; fn main() { print(a.f() + b.g()); }",
            ),
            ("a.kmpl", "import c; fn f() -> int { return c.X; }"),
            ("b.kmpl", "import c; fn g() -> int { return c.X; }"),
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
            ("main.kmpl", "import a;\nfn main() { }\n"),
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
            ("main.kmpl", "import a;\nfn main() { print(a.f(0)); }\n"),
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
            ("main.kmpl", "import a;\nfn main() { }\n"),
            ("a.kmpl", "\nfn (\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("a.kmpl:2:4"), "{}", stderr(&out));

    let out = run_project(
        "lex_error_import",
        &[
            ("main.kmpl", "import a;\nfn main() { }\n"),
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
        &[("main.kmpl", "import lib.nope;\nfn main() { }\n")],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("cannot read module `lib.nope`"), "{err}");
    assert!(err.contains("main.kmpl:1:1"), "{err}");
}

#[test]
fn import_cycles() {
    let out = run_project(
        "import_cycle",
        &[
            ("main.kmpl", "import a;\nfn main() { }\n"),
            ("a.kmpl", "import b;\n"),
            ("b.kmpl", "import a;\n"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("import cycle: a -> b -> a"), "{err}");
    assert!(err.contains("b.kmpl:1:1"), "{err}");

    let out = run_project(
        "import_self_cycle",
        &[
            ("main.kmpl", "import a;\nfn main() { }\n"),
            ("a.kmpl", "import main;\n"),
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
    assert!(err.contains("unknown module `a`, add `import a;`"), "{err}");
    assert!(err.contains("main.kmpl:1:19"), "{err}");
}
