//! Builds `kmpl_rt` so that `kmpl build` can carry it inside the binary.
//! Cargo cannot yet express "embed this other crate's static library", so
//! this runs a second Cargo with its own target directory.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let crates = manifest.parent().unwrap();
    let workspace = crates.parent().unwrap();
    for runtime_crate in ["kmpl_rt", "kmpl_builtins", "kmpl_ast"] {
        println!(
            "cargo:rerun-if-changed={}",
            crates.join(runtime_crate).display()
        );
    }
    println!("cargo:rerun-if-changed=build.rs");

    let target_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("rt");
    // Named explicitly because Nix sets `CARGO_BUILD_TARGET`, which changes
    // where the library ends up.
    let target = env::var("TARGET").unwrap();
    let status = Command::new(env::var_os("CARGO").unwrap())
        .current_dir(workspace)
        .args(["build", "--release", "--locked", "-p", "kmpl_rt"])
        .args(["--target", &target])
        .arg("--target-dir")
        .arg(&target_dir)
        // Set when this runs under `cargo clippy`; the runtime is linted on
        // its own as a workspace member.
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CLIPPY_ARGS")
        .status()
        .expect("cannot run cargo to build the runtime");
    assert!(status.success(), "building the runtime failed");

    println!(
        "cargo:rustc-env=KMPL_RT_LIB={}",
        target_dir
            .join(target)
            .join("release/libkmpl_rt.a")
            .display()
    );
}
