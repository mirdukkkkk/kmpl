fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: kmpl <file>");
        std::process::exit(2);
    });

    let src = match std::fs::read_to_string(&path) {
        Ok(src) => src,
        Err(e) => {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        }
    };

    let tokens = match kmpl_lexer::lex(&src) {
        Ok(tokens) => tokens,
        Err(span) => {
            eprintln!(
                "{path}: unexpected character at {}..{}",
                span.start, span.end
            );
            std::process::exit(1);
        }
    };

    let program = match kmpl_parse::parse(tokens) {
        Ok(program) => program,
        Err(err) => {
            eprintln!(
                "{path}: {} at {}..{}",
                err.message, err.span.start, err.span.end
            );
            std::process::exit(1);
        }
    };

    if let Err(errors) = kmpl_check::check(&program) {
        for err in &errors {
            eprintln!(
                "{path}: {} at {}..{}",
                err.message, err.span.start, err.span.end
            );
        }
        std::process::exit(1);
    }

    println!("{program:#?}");
}
