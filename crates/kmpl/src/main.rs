use std::io::Write;

use kmpl_ast::Span;

const USAGE: &str = "usage: kmpl [--ast] <file>";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (print_ast, path) = match args.as_slice() {
        [flag, path] if flag == "--ast" => (true, path),
        [path] if !path.starts_with("--") => (false, path),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };

    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) => fail(&format!("{path}: {e}")),
    };

    let tokens = match kmpl_lexer::lex(&src) {
        Ok(tokens) => tokens,
        Err(span) => fail(&located(path, "unexpected character", span)),
    };

    let mut program = match kmpl_parse::parse(tokens) {
        Ok(program) => program,
        Err(err) => fail(&located(path, &err.message, err.span)),
    };

    if let Err(errors) = kmpl_check::check(&mut program) {
        for err in &errors {
            eprintln!("{}", located(path, &err.message, err.span));
        }
        std::process::exit(1);
    }

    if print_ast {
        println!("{program:#?}");
        return;
    }

    let mut out = std::io::BufWriter::new(std::io::stdout());
    let result = kmpl_interp::run(&program, &mut out);
    // Flush first, so output printed before an error appears before it.
    if let Err(e) = out.flush() {
        fail(&format!("kmpl: cannot write output: {e}"));
    }
    if let Err(err) = result {
        fail(&located(path, &err.message, err.span));
    }
}

/// `path: message at start..end`. Line and column come with diagnostics.
fn located(path: &str, message: &str, span: Span) -> String {
    format!("{path}: {message} at {}..{}", span.start, span.end)
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
