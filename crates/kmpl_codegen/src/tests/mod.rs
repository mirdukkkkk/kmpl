//! Every test builds a real executable and runs it.

mod eval;

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use kmpl_ast::Span;

#[derive(Debug, PartialEq)]
struct RuntimeError {
    message: String,
    span: Span,
}

type Outcome = Result<String, (RuntimeError, String)>;

/// Builds and runs `src`. Returns what it printed, or the runtime error
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

fn module(src: &str, optimize: bool) -> kmpl_ir::Module {
    let mut program = kmpl_parse::parse(kmpl_lexer::lex(src).unwrap()).unwrap();
    let types = kmpl_check::check(&mut program).unwrap();
    let mut module = kmpl_lower::lower(&program, &types);
    if optimize {
        kmpl_ir::optimize(&mut module);
    }
    kmpl_ir::annotate(&mut module);
    module
}

fn run_mode(src: &str, args: &[&str], input: &[u8], optimize: bool) -> Outcome {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "kmpl-native-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));

    // The span instead of source lines, so tests can look at it.
    let locate = |span: Span| format!("{}..{}\n", span.start, span.end);
    crate::build(
        &module(src, optimize),
        &path,
        &locate,
        crate::Options::default(),
    )
    .unwrap_or_else(|message| panic!("{message} for: {src}"));

    let mut child = Command::new(&path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(&path).unwrap();

    let printed = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    match output.status.code() {
        Some(0) => Ok(printed),
        Some(1) => Err((parse_error(&stderr), printed)),
        other => panic!("exit code {other:?}, stderr {stderr} for: {src}"),
    }
}

/// `error: message`, then the line that `locate` produced.
fn parse_error(stderr: &str) -> RuntimeError {
    let (message, location) = stderr
        .strip_suffix('\n')
        .and_then(|text| text.rsplit_once('\n'))
        .unwrap_or_else(|| panic!("not an error: {stderr}"));
    let (start, end) = location.split_once("..").unwrap();
    RuntimeError {
        message: message.strip_prefix("error: ").unwrap().to_owned(),
        span: Span::new(start.parse().unwrap(), end.parse().unwrap()),
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
