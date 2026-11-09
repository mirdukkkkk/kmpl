use kmpl_ast::Span;

use super::{Sources, lex_error_message, render};

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
    let src = "x = 1e400;";
    assert_eq!(
        lex_error_message(src, span_of(src, "1e400")),
        "real literal `1e400` is out of range"
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

#[test]
fn bad_string_messages() {
    let unknown = r#"unknown escape in string, use `\n`, `\t`, `\r`, `\"` or `\\`"#;
    for src in [r#"x = "a\qb";"#, r#""\q""#] {
        let span = kmpl_lexer::lex(src).unwrap_err();
        assert_eq!(lex_error_message(src, span), unknown, "{src}");
    }

    for src in [r#"x = "abc"#, "x = \"abc\ny\";", r#""trailing\""#] {
        let span = kmpl_lexer::lex(src).unwrap_err();
        assert_eq!(lex_error_message(src, span), "unterminated string", "{src}");
    }
}

#[test]
fn sources_find_the_file_of_a_span() {
    let mut sources = Sources::default();
    assert_eq!(sources.add("a.kmpl", "first\n"), 0);
    let start = sources.add("b.kmpl", "one\ntwo\n");
    assert_eq!(start, 7);

    let out = sources.render("bad", Span::new(0, 5));
    assert!(out.contains("--> a.kmpl:1:1"), "{out}");
    let out = sources.render("bad", Span::new(start + 4, start + 7));
    assert!(out.contains("--> b.kmpl:2:1"), "{out}");
    assert!(out.contains("2 | two"), "{out}");
}

#[test]
fn sources_tell_the_end_of_a_file_from_the_next_one() {
    let mut sources = Sources::default();
    sources.add("a.kmpl", "abc");
    sources.add("b.kmpl", "def");
    let out = sources.render("bad", Span::new(3, 3));
    assert!(out.contains("--> a.kmpl:1:4"), "{out}");
}
