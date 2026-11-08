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
