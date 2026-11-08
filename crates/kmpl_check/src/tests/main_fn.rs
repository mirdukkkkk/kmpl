use super::{assert_error, errors_exact};

#[test]
fn main_is_required() {
    assert_eq!(
        errors_exact("fn f() { }"),
        vec!["no `main` function in the program"]
    );
    assert_eq!(errors_exact(""), vec!["no `main` function in the program"]);
    assert_eq!(
        errors_exact("const main: int = 1;"),
        vec!["`main` must be a function"]
    );
}

#[test]
fn main_signature() {
    assert_error("fn main(a: int) { }", "`main` must not take parameters");
    assert_error(
        "fn main() -> int { return 0; }",
        "`main` must not return a value",
    );
    assert_eq!(
        errors_exact("fn main(a: int) -> int { return a; }"),
        vec![
            "`main` must not take parameters",
            "`main` must not return a value",
        ]
    );
}
