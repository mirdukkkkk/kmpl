# kmpl

[![CI](https://github.com/mirdukkkkk/kmpl/actions/workflows/ci.yml/badge.svg)](https://github.com/mirdukkkkk/kmpl/actions/workflows/ci.yml)

kmpl (Kepler Math Programming Language) is a small statically typed language
for math, with Rust-like syntax and type inference. Complex numbers, vectors
and matrices (a built-in `matrix` type of `real`) are part of the language.

```kmpl
struct Body {
    pos: vec2,
    vel: vec2,
}

fn advance(b: Body, dt: real) -> Body {
    b.pos += b.vel * dt;
    return b;
}

fn power(m: [[real]], k: int) -> [[real]] {
    let out = m;
    for i in 1..k {
        out *= m;
    }
    return out;
}

fn main() {
    let b = advance(Body { pos: vec2(0, 0), vel: vec2(1, 2) }, 0.5);
    print(b.pos);  // (0.5, 1.0)

    print(power([[1.0, 1.0], [1.0, 0.0]], 10));  // [[89.0, 55.0], [55.0, 34.0]]
    print(sqrt(-4.0 + 0.0i));  // 0.0+2.0i
}
```

## Status

kmpl is a hobby project at version 0.3, and the language can still change.
Programs are compiled through an optimized SSA intermediate representation and
LLVM to native code for Linux on x86-64 or ARM64. `kmpl file` compiles and runs a
program, `kmpl build file` keeps the executable. Modules can come from files, from
the libraries that ship with `kmpl`, such as `std:fs`, or from a link pinned in
`kmpl.lock`.

## Install

You need a recent stable Rust and LLVM 23 (`LLVM_SYS_231_PREFIX` names its
location; the Nix dev shell sets it up). Running or building a program also
needs a C compiler named `cc` to link it.

LLVM and TLS for downloading modules are embedded in the binary. Packagers can use
the system's libraries instead with
`--no-default-features --features dynamic-llvm,dynamic-tls`.

```sh
git clone https://github.com/mirdukkkkk/kmpl
cd kmpl
cargo build --release
./target/release/kmpl examples/demo.kmpl
```

With Nix: `nix run . -- examples/demo.kmpl`.

## Usage

```text
kmpl [--ast | --ir | --llvm-ir] [-v | -vv] [--no-opt] [--locked] <file> [args...]
kmpl build [-v | -vv] [--no-opt] [--locked] <file> [-o <output>]
kmpl add <link or path> [name]
kmpl update [name [version]] [--dry-run]
kmpl vendor
```

Everything after the file is passed to the program. `-v` shows the time of each
compiler stage and `--llvm-ir` prints the LLVM IR. `kmpl --help` lists the options.

## Learn the language

[`docs/language.md`](docs/language.md) is a short tour of the whole language.
[`examples/`](examples) has working programs. Good ones to start with are
[`structs`](examples/structs.kmpl), [`matrix`](examples/matrix.kmpl), [`matrices`](examples/matrices.kmpl),
[`generics`](examples/generics.kmpl), [`enums`](examples/enums.kmpl),
[`linalg`](examples/linalg.kmpl), [`modules`](examples/modules.kmpl) and
[`sum`](examples/sum.kmpl), which reads arguments and standard input.

## Development

`nix develop` gives you the right Rust and LLVM.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE)
or [MIT license](LICENSE-MIT) at your option.
