//! The program must have `fn main()` with no parameters and no return type.

use kmpl_ast::{Item, Program, Span};

use crate::error::CheckError;

pub fn check_main(program: &Program) -> Vec<CheckError> {
    let main = program.items.iter().find_map(|item| match item {
        Item::Function(f) if f.name.name == "main" => Some(Ok(f)),
        Item::Const(c) if c.name.name == "main" => Some(Err(c)),
        _ => None,
    });

    let mut errors = Vec::new();
    match main {
        None => errors.push(CheckError::new(
            "no `main` function in the program",
            Span::new(0, 0),
        )),
        Some(Err(c)) => errors.push(CheckError::new("`main` must be a function", c.name.span)),
        Some(Ok(f)) => {
            if !f.params.is_empty() {
                errors.push(CheckError::new(
                    "`main` must not take parameters",
                    f.name.span,
                ));
            }
            if f.ret.is_some() {
                errors.push(CheckError::new(
                    "`main` must not return a value",
                    f.name.span,
                ));
            }
        }
    }
    errors
}
