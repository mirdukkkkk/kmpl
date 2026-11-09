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

/// Builds `src` and runs the result.
fn build_and_run(test: &str, src: &str) -> Output {
    let dir = project(test, src);
    let out = kmpl_in(&dir, &["build", "program.kmpl", "-o", "made"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    Command::new(dir.join("made")).output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn numbers_branches_loops_and_calls() {
    let out = build_and_run(
        "numbers",
        "const K: int = 3;
        fn fib(n: int) -> int {
            if n < 2 { return n; }
            return fib(n - 1) + fib(n - 2);
        }
        fn main() {
            let sum = 0.0;
            for i in 0..=4 { sum += sqrt(real(i * i)); }
            print(sum);
            print(fib(10) * K);
            print(7 / -2);
            print(1.0 / 0.0 > 1e308 && !false);
        }",
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(stdout(&out), "10.0\n165\n-3\ntrue\n");
}

#[test]
fn a_runtime_error_looks_like_the_vm_one_after_flushing_output() {
    let out = build_and_run(
        "division",
        "fn main() {\n    print(1);\n    let z = 0;\n    print(1 / z);\n}\n",
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "1\n");
    assert_eq!(
        stderr(&out),
        "error: division by zero\n --> program.kmpl:4:11\n  |\n4 |     print(1 / z);\n  |           ^^^^^\n"
    );
}

#[test]
fn integer_overflow_is_an_error() {
    let out = build_and_run(
        "overflow",
        "fn main() { let a = 9223372036854775807; print(a + 1); }",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).starts_with("error: integer overflow\n"),
        "{out:?}"
    );
}

#[test]
fn the_call_depth_limit_matches_the_vm() {
    let out = build_and_run(
        "depth",
        "fn down(n: int) -> int { return down(n + 1); } fn main() { print(down(0)); }",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).starts_with("error: stack overflow: more than 10000 nested calls\n"),
        "{out:?}"
    );
}

#[test]
fn a_failing_built_in_reports_its_error() {
    let out = build_and_run("builtin", "fn main() { print(int(0.0 / 0.0)); }");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("--> program.kmpl:1:"), "{out:?}");
}

#[test]
fn an_executable_is_stripped_and_names_the_compiler() {
    let dir = project("metadata", "fn main() { }");
    let out = kmpl_in(&dir, &["build", "program.kmpl", "-o", "made"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");

    let bytes = std::fs::read(dir.join("made")).unwrap();
    let contains = |text: &str| bytes.windows(text.len()).any(|w| w == text.as_bytes());
    assert!(contains(concat!("kmpl ", env!("CARGO_PKG_VERSION"), "\0")));
    assert!(!contains(".symtab"), "the symbol table is still there");
}
