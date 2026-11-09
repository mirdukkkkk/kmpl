mod diagnostics;
mod loader;

use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use diagnostics::Sources;

const USAGE: &str = "usage: kmpl [--ast | --ir] [--no-opt] <file> [args...]
       kmpl build [--no-opt] <file> [-o <output>]";

const HELP: &str = "\
kmpl: the Kepler Math Programming Language

Usage:
    kmpl [--ast | --ir] [--no-opt] <file> [args...]
    kmpl build [--no-opt] <file> [-o <output>]

Running or building a program needs a C compiler named `cc` to link it.

Commands:
    build        compile the program to a native executable and keep it

Arguments:
    <file>       the program to compile and run, a .kmpl file with `fn main()`
    [args...]    passed to the program, which reads them with `args()`

Options:
    --ast        check the program and print its syntax tree, without running it
    --ir         check the program and print its intermediate representation
    --no-opt     do not optimize the intermediate representation
    -o <output>  where `build` writes the executable, by default the name of
                 the file without `.kmpl`, in the current directory
    -h, --help   show this help

Examples:
    kmpl examples/demo.kmpl
    kmpl program.kmpl 1 2 3
    echo 42 | kmpl program.kmpl
    kmpl build program.kmpl -o program
";

/// Stack for the whole compiler. Compiling a long expression recurses. Only
/// touched pages are actually allocated.
const STACK_SIZE: usize = 1 << 30;

fn main() {
    let worker = std::thread::Builder::new()
        .name("kmpl".to_owned())
        .stack_size(STACK_SIZE)
        .spawn(run)
        .expect("cannot start the main thread");
    if let Err(panic) = worker.join() {
        std::panic::resume_unwind(panic);
    }
}

fn run() {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    let (building, mut rest) = match args.split_first() {
        Some((command, tail)) if command == "build" => (true, tail),
        _ => (false, args.as_slice()),
    };
    let mut print_flag = None;
    let mut optimize = true;
    while let [flag, tail @ ..] = rest {
        match flag.as_str() {
            "--ast" | "--ir" if !building => print_flag = Some(flag.as_str()),
            "--no-opt" => optimize = false,
            _ => break,
        }
        rest = tail;
    }
    if matches!(rest.first().map(String::as_str), Some("--help" | "-h")) {
        print!("{HELP}");
        return;
    }
    // Everything after the file belongs to the program.
    let [path, program_args @ ..] = rest else {
        usage();
    };
    if path.starts_with("--") {
        usage();
    }
    let output = building.then(|| build_output(path, program_args));

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

    let types = match kmpl_check::check(&mut program) {
        Ok(types) => types,
        Err(errors) => fail(&render_all(errors)),
    };

    if print_flag == Some("--ast") {
        println!("{program:#?}");
        return;
    }

    let mut module = kmpl_lower::lower(&program, &types);
    if optimize {
        kmpl_ir::optimize(&mut module);
    }
    if print_flag == Some("--ir") {
        print!("{module}");
        return;
    }
    kmpl_ir::annotate(&mut module);

    let locate = |span| sources.location(span);
    if let Some(output) = output {
        if let Err(message) = kmpl_codegen::build(&module, &output, &locate) {
            fail(&message);
        }
        return;
    }

    match kmpl_codegen::run(&module, &locate, program_args) {
        Ok(status) => std::process::exit(exit_code(status)),
        Err(message) => fail(&message),
    }
}

/// The program's own exit code, or the shell's convention if a signal ended it.
fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// Where `kmpl build` writes: `-o <output>` after the file, or else the name
/// of the file without its extension, in the current directory.
fn build_output(path: &str, after_file: &[String]) -> PathBuf {
    match after_file {
        [] => match Path::new(path).file_stem() {
            Some(stem) => PathBuf::from(stem),
            None => usage(),
        },
        [flag, output] if flag == "-o" => PathBuf::from(output),
        _ => usage(),
    }
}

fn usage() -> ! {
    eprintln!("{USAGE}");
    std::process::exit(2);
}

/// Prints `message` (a rendered error or a plain line) and exits with 1.
fn fail(message: &str) -> ! {
    eprint!("{message}");
    if !message.ends_with('\n') {
        eprintln!();
    }
    std::process::exit(1);
}
