# kmpl

[![CI](https://github.com/mirdukkkkk/kmpl/actions/workflows/ci.yml/badge.svg)](https://github.com/mirdukkkkk/kmpl/actions/workflows/ci.yml)

kmpl (Kepler Math Programming Language) is a small statically typed language
for math, with Rust-like syntax and type inference. Complex numbers, vectors
and matrices are part of the language.

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

kmpl is a hobby project at version 0.1, and the language can still change.
Programs are compiled to an optimized SSA intermediate representation. `kmpl file`
runs it on a virtual machine, and `kmpl build file` compiles it through LLVM to a
native executable for Linux on x86-64 or ARM64. There is no file I/O yet, only
standard input and output.

## Install

You need a recent stable Rust and LLVM 23 (`LLVM_SYS_231_PREFIX` names its
location; the Nix dev shell sets it up). `kmpl build` also needs a C compiler
named `cc` to link.

```sh
git clone https://github.com/mirdukkkkk/kmpl
cd kmpl
cargo build --release
./target/release/kmpl examples/demo.kmpl
```

With Nix: `nix run . -- examples/demo.kmpl`.

## Usage

```text
kmpl [--ast | --ir] [--no-opt] <file> [args...]
kmpl build [--no-opt] <file> [-o <output>]
```

Everything after the file is passed to the program. `kmpl --help` lists the
options.

## Learn the language

[`docs/language.md`](docs/language.md) is a short tour of the whole language.
[`examples/`](examples) has working programs. Good ones to start with are
[`structs`](examples/structs.kmpl), [`matrices`](examples/matrices.kmpl),
[`generics`](examples/generics.kmpl) and [`sum`](examples/sum.kmpl), which reads
arguments and standard input.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE)
or [MIT license](LICENSE-MIT) at your option.
