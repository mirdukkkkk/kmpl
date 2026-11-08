mod eval;
mod ops;

/// Checks and runs `src`. Returns what it printed, or the runtime error
/// message with what was printed before it.
fn run(src: &str) -> Result<String, (String, String)> {
    run_with(src, &[], b"")
}

/// `run` with command line arguments and standard input.
fn run_with(src: &str, args: &[&str], input: &[u8]) -> Result<String, (String, String)> {
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    let types = kmpl_check::check(&mut program).unwrap();
    let module = kmpl_lower::lower(&program, &types);

    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let mut out = Vec::new();
    let mut input = input;
    let io = crate::Io {
        out: &mut out,
        input: &mut input,
        args: &args,
    };
    let result = crate::run(&module, io);
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
