//! Test helpers shared by the per-module test files.

mod generics;
mod infer;
mod init;
mod literals;
mod main_fn;
mod modules;
mod resolve;
mod returns;
mod type_info;
mod types;

use crate::check;

/// Error messages for `src`, in the order they were found.
/// Adds an empty `fn main() { }` if `src` has no `main`, so tests
/// about other rules do not also get "no `main`".
fn errors(src: &str) -> Vec<String> {
    let src = if src.contains("main") {
        src.to_owned()
    } else {
        format!("{src} fn main() {{ }}")
    };
    errors_exact(&src)
}

/// Error messages for `src` exactly as written.
fn errors_exact(src: &str) -> Vec<String> {
    let tokens = kmpl_lexer::lex(src).unwrap();
    let program = kmpl_parse::parse(tokens).unwrap();
    let mut program = program;
    match check(&mut program) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
    }
}

fn assert_ok(src: &str) {
    assert_eq!(errors(src), Vec::<String>::new(), "source: {src}");
}

/// `src` must give exactly one error, with this message.
fn assert_error(src: &str, message: &str) {
    assert_eq!(errors(src), vec![message.to_owned()], "source: {src}");
}
