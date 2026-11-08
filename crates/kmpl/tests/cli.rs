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
    assert!(stderr(&out).contains("usage: kmpl <file>"));
}

#[test]
fn missing_file() {
    let out = kmpl(&["/nonexistent/x.kmpl".as_ref()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).starts_with("/nonexistent/x.kmpl: "));
}

#[test]
fn lex_error() {
    let out = run("lex_error", "fn main() { let x = 1 $ 2; }");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).ends_with("unexpected character at 22..23\n"));
}

#[test]
fn parse_error() {
    let out = run("parse_error", "fn main() {\n    let x = 1\n}\n");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).ends_with("expected `;`, found `}` at 26..27\n"));
}

#[test]
fn check_errors_are_all_printed() {
    let out = run("check_errors", "fn main() { a = 1; b(); }");
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert_eq!(err.lines().count(), 2, "{err}");
    assert!(err.contains("undefined variable `a` at 12..13"));
    assert!(err.contains("undefined function `b` at 19..20"));
}

#[test]
fn valid_program_prints_ast() {
    let out = run("valid", "fn main() { print(1); }");
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).starts_with("Program {"));
    assert!(stderr(&out).is_empty());
}
