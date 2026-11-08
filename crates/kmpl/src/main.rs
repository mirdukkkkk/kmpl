mod diagnostics;
mod loader;

use std::io::Write;

use diagnostics::Sources;

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

    let mut sources = Sources::default();
    let modules = match loader::load(path, &mut sources) {
        Ok(modules) => modules,
        Err(message) => fail(&message),
    };

    let render_all = |errors: Vec<kmpl_check::CheckError>| -> String {
        let rendered: Vec<String> = errors
            .iter()
            .map(|err| sources.render(&err.message, err.span))
            .collect();
        // A blank line between errors.
        rendered.join("\n")
    };

    let mut program = match kmpl_check::link(modules) {
        Ok(program) => program,
        Err(errors) => fail(&render_all(errors)),
    };

    if let Err(errors) = kmpl_check::check(&mut program) {
        fail(&render_all(errors));
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
        fail(&sources.render(&err.message, err.span));
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
