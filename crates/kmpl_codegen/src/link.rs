//! Joins the object file and the runtime with the system linker.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static RUNTIME: &[u8] = include_bytes!(env!("KMPL_RT_LIB"));

/// What a Rust static library needs from the system on Linux.
const SYSTEM_LIBS: [&str; 7] = [
    "-lgcc_s",
    "-lutil",
    "-lrt",
    "-lpthread",
    "-lm",
    "-ldl",
    "-lc",
];

/// A private directory, deleted when this goes out of scope.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<TempDir, String> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "kmpl-build-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir(&path)
            .map_err(|e| format!("kmpl build: cannot create {}: {e}", path.display()))?;
        Ok(TempDir(path))
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let path = self.0.join(name);
        std::fs::write(&path, bytes)
            .map_err(|e| format!("kmpl build: cannot write {}: {e}", path.display()))?;
        Ok(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Only temporary files are lost if this fails.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn link(object: &[u8], output: &Path) -> Result<(), String> {
    let dir = TempDir::new()?;
    let object = dir.write("program.o", object)?;
    let runtime = dir.write("libkmpl_rt.a", RUNTIME)?;

    let result = Command::new("cc")
        .arg(&object)
        .arg(&runtime)
        .arg("-o")
        .arg(output)
        .arg("-Wl,--gc-sections")
        .args(SYSTEM_LIBS)
        .output()
        .map_err(|e| format!("kmpl build: cannot run the linker `cc`: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "kmpl build: linking failed:\n{}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    Ok(())
}
