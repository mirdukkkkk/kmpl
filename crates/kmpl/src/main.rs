mod diagnostics;

use std::io::Write;

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
        Err(span) => {
            let message = diagnostics::lex_error_message(&src, span);
            fail(&diagnostics::render(path, &src, &message, span))
        }
    };

    let mut program = match kmpl_parse::parse(tokens) {
        Ok(program) => program,
        Err(err) => fail(&diagnostics::render(path, &src, &err.message, err.span)),
    };

    if let Err(errors) = kmpl_check::check(&mut program) {
        let rendered: Vec<String> = errors
            .iter()
            .map(|err| diagnostics::render(path, &src, &err.message, err.span))
            .collect();
        // A blank line between errors.
        eprint!("{}", rendered.join("\n"));
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
        fail(&diagnostics::render(path, &src, &err.message, err.span));
    }
}

/// Prints `message` (a rendered error or a plain line) and exits with 1.
fn fail(message: &str) -> ! {
    eprint!("{message}");
    if !message.ends_with('\n') {
        eprintln!();
    }
    std::process::exit(1);
}
