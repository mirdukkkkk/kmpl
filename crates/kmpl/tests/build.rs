//! `kmpl build`: makes real executables and runs them.

use std::path::PathBuf;
use std::process::{Command, Output};

/// A fresh directory for one test, holding `src` as `program.kmpl`.
fn project(test: &str, src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kmpl-build-test-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("program.kmpl"), src).unwrap();
    dir
}

fn kmpl_in(dir: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kmpl"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn an_empty_program_builds_and_runs() {
    let dir = project("empty", "fn main() { }");
    let out = kmpl_in(&dir, &["build", "program.kmpl", "-o", "made"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    let run = Command::new(dir.join("made")).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert!(run.stdout.is_empty());
}

#[test]
fn the_default_output_is_named_after_the_file() {
    let dir = project("default_name", "fn main() { }");
    let out = kmpl_in(&dir, &["build", "program.kmpl"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(dir.join("program").is_file());
}

#[test]
fn a_program_with_errors_produces_no_file() {
    let dir = project("errors", "fn main() { let x: int = true; }");
    let out = kmpl_in(&dir, &["build", "program.kmpl", "-o", "made"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!dir.join("made").exists());
}

#[test]
fn extra_arguments_are_a_usage_error() {
    let dir = project("usage", "fn main() { }");
    let out = kmpl_in(&dir, &["build", "program.kmpl", "-o"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("kmpl build"));
}
