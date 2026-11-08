//! Runs every `examples/*.kmpl` and compares its output with the
//! `.out` file next to it.

use std::path::Path;
use std::process::Command;

#[test]
fn examples_match_expected_output() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut sources: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "kmpl"))
        .collect();
    sources.sort();
    assert!(!sources.is_empty(), "no examples in {}", dir.display());

    // Examples may write files; keep them out of the repository.
    let work_dir = std::env::temp_dir().join(format!("kmpl_examples_{}", std::process::id()));
    std::fs::create_dir_all(&work_dir).unwrap();

    let mut failures = Vec::new();
    for source in &sources {
        let name = source.file_name().unwrap().to_string_lossy();
        let expected = match std::fs::read_to_string(source.with_extension("out")) {
            Ok(text) => text,
            Err(_) => {
                failures.push(format!("{name}: missing .out file"));
                continue;
            }
        };

        let out = Command::new(env!("CARGO_BIN_EXE_kmpl"))
            .arg(source)
            .current_dir(&work_dir)
            .output()
            .unwrap();
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

    std::fs::remove_dir_all(&work_dir).unwrap();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
