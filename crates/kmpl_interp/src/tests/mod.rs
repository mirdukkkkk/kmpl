mod eval;
mod ops;

/// Checks and runs `src`. Returns what it printed, or the runtime error
/// message with what was printed before it.
fn run(src: &str) -> Result<String, (String, String)> {
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    kmpl_check::check(&mut program).unwrap();

    let mut out = Vec::new();
    let result = crate::run(&program, &mut out);
    let printed = String::from_utf8(out).unwrap();
    match result {
        Ok(()) => Ok(printed),
        Err(e) => Err((e.message, printed)),
    }
}

/// Output of `fn main() { body }`.
fn output(body: &str) -> String {
    run(&format!("fn main() {{ {body} }}")).unwrap()
}

/// Runtime error message of `src`.
fn runtime_error(src: &str) -> String {
    run(src).unwrap_err().0
}
