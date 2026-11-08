use kmpl_ast::Span;

use super::{lex_error_message, render};

/// Span of the first occurrence of `part` in `src`.
fn span_of(src: &str, part: &str) -> Span {
    let start = src.find(part).unwrap();
    Span::new(start, start + part.len())
}

#[test]
fn first_line() {
    let src = "let x = 1 $ 2;";
    assert_eq!(
        render("a.kmpl", src, "unexpected character `$`", span_of(src, "$")),
        "error: unexpected character `$`\n\
         \x20--> a.kmpl:1:11\n\
         \x20 |\n\
         1 | let x = 1 $ 2;\n\
         \x20 |           ^\n"
    );
}

#[test]
fn later_line_and_wide_span() {
    let src = "fn main() {\n    let z = 0;\n    print(1 / z);\n}\n";
    assert_eq!(
        render("x.kmpl", src, "division by zero", span_of(src, "1 / z")),
        "error: division by zero\n\
         \x20--> x.kmpl:3:11\n\
         \x20 |\n\
         3 |     print(1 / z);\n\
         \x20 |           ^^^^^\n"
    );
}

#[test]
fn wide_line_numbers_align() {
    let src = format!("{}let y = q;\n", "\n".repeat(11));
    let out = render("f.kmpl", &src, "undefined variable `q`", span_of(&src, "q"));
    assert_eq!(
        out,
        "error: undefined variable `q`\n\
         \x20 --> f.kmpl:12:9\n\
         \x20  |\n\
         12 | let y = q;\n\
         \x20  |         ^\n"
    );
}

#[test]
fn columns_count_characters_not_bytes() {
    let src = "// привет\nlet ж = 1;";
    let out = render("u.kmpl", src, "msg", span_of(src, "ж"));
    assert!(out.contains("--> u.kmpl:2:5\n"), "{out}");
    assert!(out.ends_with("2 | let ж = 1;\n  |     ^\n"), "{out}");

    let src = "let s = \"ё\" + bad;";
    let out = render("u.kmpl", src, "msg", span_of(src, "bad"));
    assert!(out.contains(":1:15\n"), "{out}");
    assert!(out.ends_with("|               ^^^\n"), "{out}");
}

#[test]
fn tabs_are_kept_under_the_line() {
    let src = "\tlet x = y;";
    let out = render("t.kmpl", src, "msg", span_of(src, "y"));
    assert!(
        out.ends_with("1 | \tlet x = y;\n  | \t        ^\n"),
        "{out}"
    );
}

#[test]
fn crlf_line_endings() {
    let src = "a;\r\nb c;\r\n";
    let out = render("w.kmpl", src, "msg", span_of(src, "c"));
    assert!(out.contains(":2:3\n"), "{out}");
    assert!(out.ends_with("2 | b c;\n  |   ^\n"), "{out}");
}

#[test]
fn empty_span_at_end_of_file() {
    let src = "fn main() {\n    let x = 1;";
    let end = src.len();
    let out = render("e.kmpl", src, "expected `}`", Span::new(end, end));
    assert!(out.contains("--> e.kmpl:2:15\n"), "{out}");
    assert!(
        out.ends_with("2 |     let x = 1;\n  |               ^\n"),
        "{out}"
    );
}

#[test]
fn span_over_several_lines_underlines_first_line() {
    let src = "x = foo(1,\n  2);";
    let out = render("m.kmpl", src, "msg", span_of(src, "foo(1,\n  2)"));
    assert!(out.ends_with("1 | x = foo(1,\n  |     ^^^^^^\n"), "{out}");
}

#[test]
fn empty_source() {
    let out = render(
        "n.kmpl",
        "",
        "no `main` function in the program",
        Span::new(0, 0),
    );
    assert!(out.contains("--> n.kmpl:1:1\n"), "{out}");
}

#[test]
fn span_inside_a_multibyte_character_does_not_panic() {
    let src = "ж";
    let out = render("b.kmpl", src, "msg", Span::new(1, 2));
    assert!(out.contains(":1:1\n"), "{out}");
}

#[test]
fn lex_messages() {
    let src = "let x = 99999999999999999999;";
    assert_eq!(
        lex_error_message(src, span_of(src, "99999999999999999999")),
        "integer literal `99999999999999999999` is too large for `int`"
    );
    let src = "a $ b";
    assert_eq!(
        lex_error_message(src, span_of(src, "$")),
        "unexpected character `$`"
    );
    let src = "a ж b";
    assert_eq!(
        lex_error_message(src, span_of(src, "ж")),
        "unexpected character `ж`"
    );
}
