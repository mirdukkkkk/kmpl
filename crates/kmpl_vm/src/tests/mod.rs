mod eval;

/// Checks and runs `src`. Returns what it printed, or the runtime error
/// message with what was printed before it.
fn run(src: &str) -> Result<String, (String, String)> {
    run_with(src, &[], b"")
}

/// `run` with command line arguments and standard input. Runs the program
/// with and without optimizations: they must not change anything.
fn run_with(src: &str, args: &[&str], input: &[u8]) -> Result<String, (String, String)> {
    let optimized = run_mode(src, args, input, true);
    let plain = run_mode(src, args, input, false);
    assert_eq!(
        optimized, plain,
        "optimizations changed the result of: {src}"
    );
    optimized.map_err(|(error, printed)| (error.message, printed))
}

fn run_mode(
    src: &str,
    args: &[&str],
    input: &[u8],
    optimize: bool,
) -> Result<String, (crate::RuntimeError, String)> {
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    let types = kmpl_check::check(&mut program).unwrap();
    let mut module = kmpl_lower::lower(&program, &types);
    if optimize {
        kmpl_ir::optimize(&mut module);
    }
    kmpl_ir::annotate(&mut module);

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
        Err(e) => Err((e, printed)),
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
