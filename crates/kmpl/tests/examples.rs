//! Runs every `examples/*.kmpl`, directly and as a built executable, and
//! compares its output with the `.out` file next to it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn sources() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut sources: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "kmpl"))
        .collect();
    sources.sort();
    assert!(!sources.is_empty(), "no examples in {}", dir.display());
    sources
}

/// Compares what `run` produces for every example with its `.out` file.
fn check_examples(run: impl Fn(&Path) -> Output) {
    let mut failures = Vec::new();
    for source in &sources() {
        let name = source.file_name().unwrap().to_string_lossy();
        let expected = match std::fs::read_to_string(source.with_extension("out")) {
            Ok(text) => text,
            Err(_) => {
                failures.push(format!("{name}: missing .out file"));
                continue;
            }
        };

        let out = run(source);
        let actual = String::from_utf8_lossy(&out.stdout);

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            failures.push(format!("{name}: exit {:?}\n{stderr}", out.status.code()));
        } else if actual != expected {
            failures.push(format!(
                "{name}: output differs\n--- expected\n{expected}--- actual\n{actual}"
            ));
        }
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn examples_match_expected_output() {
    check_examples(|source| {
        Command::new(env!("CARGO_BIN_EXE_kmpl"))
            .arg(source)
            .output()
            .unwrap()
    });
}

#[test]
fn built_examples_match_expected_output() {
    let dir = std::env::temp_dir().join(format!("kmpl-built-examples-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    check_examples(|source| {
        let executable = dir.join(source.file_stem().unwrap());
        let build = Command::new(env!("CARGO_BIN_EXE_kmpl"))
            .arg("build")
            .arg(source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        if !build.status.success() {
            return build;
        }
        Command::new(&executable).output().unwrap()
    });
}
