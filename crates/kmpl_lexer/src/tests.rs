use kmpl_ast::Span;

use crate::{Token, lex};

fn tokens(src: &str) -> Vec<Token> {
    lex(src).unwrap().into_iter().map(|(t, _)| t).collect()
}

#[test]
fn let_statement() {
    assert_eq!(
        tokens("let x = 1 + 2;"),
        vec![
            Token::Let,
            Token::Ident("x".into()),
            Token::Assign,
            Token::Int(1),
            Token::Plus,
            Token::Int(2),
            Token::Semi,
        ]
    );
}

#[test]
fn keyword_vs_ident() {
    assert_eq!(
        tokens("let letter"),
        vec![Token::Let, Token::Ident("letter".into())]
    );
}

#[test]
fn numbers() {
    assert_eq!(
        tokens("1 1.5 2.0i"),
        vec![Token::Int(1), Token::Real(1.5), Token::Imag(2.0)]
    );
}

#[test]
fn two_char_operators() {
    assert_eq!(
        tokens("<= >= == != ->"),
        vec![
            Token::Le,
            Token::Ge,
            Token::EqEq,
            Token::NotEq,
            Token::Arrow
        ]
    );
}

#[test]
fn swizzle_tokens() {
    assert_eq!(
        tokens("v:zyx"),
        vec![
            Token::Ident("v".into()),
            Token::Colon,
            Token::Ident("zyx".into()),
        ]
    );
}

#[test]
fn comments_are_skipped() {
    assert_eq!(
        tokens("1 // комментарий\n2"),
        vec![Token::Int(1), Token::Int(2)]
    );
}

#[test]
fn bad_char_gives_span() {
    assert_eq!(lex("let x = 1 $ 2;"), Err(Span::new(10, 11)));
}

#[test]
fn logical_operators() {
    assert_eq!(
        tokens("a && b || c"),
        vec![
            Token::Ident("a".into()),
            Token::And,
            Token::Ident("b".into()),
            Token::Or,
            Token::Ident("c".into()),
        ]
    );
}

#[test]
fn single_ampersand_is_an_error() {
    assert_eq!(lex("a & b"), Err(Span::new(2, 3)));
}

#[test]
fn single_pipe_for_bounds() {
    assert_eq!(
        tokens("int | real || x"),
        vec![
            Token::Ident("int".into()),
            Token::Pipe,
            Token::Ident("real".into()),
            Token::Or,
            Token::Ident("x".into()),
        ]
    );
}

#[test]
fn numbers_are_64_bit() {
    assert_eq!(
        tokens("9000000000 3.141592653589793"),
        vec![Token::Int(9_000_000_000), Token::Real(std::f64::consts::PI)]
    );
}

#[test]
fn reals_without_leading_zero() {
    assert_eq!(
        tokens(".1 .5i 0.25"),
        vec![Token::Real(0.1), Token::Imag(0.5), Token::Real(0.25)]
    );
    assert_eq!(
        tokens("x-.5"),
        vec![Token::Ident("x".into()), Token::Minus, Token::Real(0.5)]
    );
}

#[test]
fn lone_dot_is_an_error() {
    assert_eq!(lex("1 . 2"), Err(Span::new(2, 3)));
}

fn spans(src: &str) -> Vec<(usize, usize)> {
    lex(src)
        .unwrap()
        .into_iter()
        .map(|(_, s)| (s.start, s.end))
        .collect()
}

#[test]
fn empty_and_blank_sources() {
    assert_eq!(tokens(""), vec![]);
    assert_eq!(tokens(" \t\r\n  \n"), vec![]);
    assert_eq!(tokens("// only a comment"), vec![]);
}

#[test]
fn all_keywords() {
    assert_eq!(
        tokens("const fn let if else while break continue return true false"),
        vec![
            Token::Const,
            Token::Fn,
            Token::Let,
            Token::If,
            Token::Else,
            Token::While,
            Token::Break,
            Token::Continue,
            Token::Return,
            Token::True,
            Token::False,
        ]
    );
}

#[test]
fn type_names_are_identifiers() {
    assert_eq!(
        tokens("int real bool complex vec2"),
        vec![
            Token::Ident("int".into()),
            Token::Ident("real".into()),
            Token::Ident("bool".into()),
            Token::Ident("complex".into()),
            Token::Ident("vec2".into()),
        ]
    );
}

#[test]
fn all_single_char_tokens() {
    assert_eq!(
        tokens("+ - * / ! < > = ( ) { } , ; :"),
        vec![
            Token::Plus,
            Token::Minus,
            Token::Star,
            Token::Slash,
            Token::Bang,
            Token::Lt,
            Token::Gt,
            Token::Assign,
            Token::LParen,
            Token::RParen,
            Token::LBrace,
            Token::RBrace,
            Token::Comma,
            Token::Semi,
            Token::Colon,
        ]
    );
}

#[test]
fn longest_match_without_spaces() {
    assert_eq!(
        tokens("a<=b==c!=d->e"),
        vec![
            Token::Ident("a".into()),
            Token::Le,
            Token::Ident("b".into()),
            Token::EqEq,
            Token::Ident("c".into()),
            Token::NotEq,
            Token::Ident("d".into()),
            Token::Arrow,
            Token::Ident("e".into()),
        ]
    );
    assert_eq!(tokens("=="), vec![Token::EqEq]);
    assert_eq!(tokens("= ="), vec![Token::Assign, Token::Assign]);
    assert_eq!(tokens("!!"), vec![Token::Bang, Token::Bang]);
}

#[test]
fn identifiers() {
    assert_eq!(
        tokens("_ _x x_1 camelCase SNAKE_CASE"),
        vec![
            Token::Ident("_".into()),
            Token::Ident("_x".into()),
            Token::Ident("x_1".into()),
            Token::Ident("camelCase".into()),
            Token::Ident("SNAKE_CASE".into()),
        ]
    );
}

#[test]
fn keyword_prefixes_are_identifiers() {
    assert_eq!(
        tokens("iff fnx lets truely returned"),
        vec![
            Token::Ident("iff".into()),
            Token::Ident("fnx".into()),
            Token::Ident("lets".into()),
            Token::Ident("truely".into()),
            Token::Ident("returned".into()),
        ]
    );
}

#[test]
fn number_edge_cases() {
    assert_eq!(tokens("0 007"), vec![Token::Int(0), Token::Int(7)]);
    // A number directly followed by letters is two tokens.
    assert_eq!(tokens("2x"), vec![Token::Int(2), Token::Ident("x".into())]);
    // `i` makes an imaginary literal only after a real.
    assert_eq!(tokens("2i"), vec![Token::Int(2), Token::Ident("i".into())]);
    assert_eq!(tokens("1.5.2"), vec![Token::Real(1.5), Token::Real(0.2)]);
    assert_eq!(tokens("-3"), vec![Token::Minus, Token::Int(3)]);
}

#[test]
fn int_limits() {
    assert_eq!(tokens("9223372036854775807"), vec![Token::Int(i64::MAX)]);
    // One past i64::MAX does not parse, so it is a lex error.
    assert!(lex("9223372036854775808").is_err());
}

#[test]
fn comment_edge_cases() {
    assert_eq!(tokens("1 // a // b\n2"), vec![Token::Int(1), Token::Int(2)]);
    assert_eq!(tokens("1 // let x = 2;"), vec![Token::Int(1)]);
    assert_eq!(tokens("1//x\n//y\n2"), vec![Token::Int(1), Token::Int(2)]);
    // A single `/` is division, not a comment.
    assert_eq!(
        tokens("a / b"),
        vec![
            Token::Ident("a".into()),
            Token::Slash,
            Token::Ident("b".into())
        ]
    );
}

#[test]
fn token_spans_are_byte_offsets() {
    assert_eq!(
        spans("let x = 10;"),
        vec![(0, 3), (4, 5), (6, 7), (8, 10), (10, 11)]
    );
    assert_eq!(spans("\t<=\r\n->"), vec![(1, 3), (5, 7)]);
    // Multi-byte characters inside a comment still count in bytes.
    assert_eq!(spans("// ж\n1"), vec![(6, 7)]);
}

#[test]
fn error_span_after_valid_tokens() {
    assert_eq!(lex("let @"), Err(Span::new(4, 5)));
    assert_eq!(lex("#"), Err(Span::new(0, 1)));
}

/// `Display` prints punctuation and keywords as source text, so lexing
/// that text gives the same token back.
#[test]
fn display_round_trips_for_fixed_tokens() {
    let src = "const fn let if else while break continue return true false \
               + - * / ! && || | == != < <= > >= = -> ( ) { } , ; :";
    for tok in tokens(src) {
        let text = tok.to_string();
        let bare = text.trim_matches('`');
        assert_eq!(tokens(bare), vec![tok.clone()], "display of {tok:?}");
    }
}

#[test]
fn display_of_value_tokens() {
    assert_eq!(Token::Ident("x".into()).to_string(), "identifier `x`");
    assert_eq!(Token::Int(42).to_string(), "number `42`");
    assert_eq!(Token::Real(1.5).to_string(), "number `1.5`");
    assert_eq!(Token::Real(1.0).to_string(), "number `1.0`");
    assert_eq!(Token::Imag(2.0).to_string(), "number `2.0i`");
}

#[test]
fn exponent_notation() {
    assert_eq!(
        tokens("1e6 1.5e-3 .5E+2 2E0 7e2i"),
        vec![
            Token::Real(1e6),
            Token::Real(1.5e-3),
            Token::Real(50.0),
            Token::Real(2.0),
            Token::Imag(700.0),
        ]
    );
}

#[test]
fn exponent_needs_digits() {
    // `2e` and `2e+` are not reals: the `e` is an identifier.
    assert_eq!(tokens("2e"), vec![Token::Int(2), Token::Ident("e".into())]);
    assert_eq!(
        tokens("2e+x"),
        vec![
            Token::Int(2),
            Token::Ident("e".into()),
            Token::Plus,
            Token::Ident("x".into()),
        ]
    );
    // An identifier that starts with `e` after an expression still works.
    assert_eq!(
        tokens("x*e1"),
        vec![
            Token::Ident("x".into()),
            Token::Star,
            Token::Ident("e1".into())
        ]
    );
}

#[test]
fn real_out_of_range() {
    assert_eq!(lex("1e400"), Err(Span::new(0, 5)));
    assert_eq!(lex("x = 1.0e309i"), Err(Span::new(4, 12)));
    // Too small rounds to zero, like in Rust and C.
    assert_eq!(tokens("1e-400"), vec![Token::Real(0.0)]);
}

#[test]
fn for_tokens() {
    assert_eq!(
        tokens("for i in 0..n"),
        vec![
            Token::For,
            Token::Ident("i".into()),
            Token::In,
            Token::Int(0),
            Token::DotDot,
            Token::Ident("n".into()),
        ]
    );
    assert_eq!(
        tokens("format inner"),
        vec![Token::Ident("format".into()), Token::Ident("inner".into())]
    );
}

#[test]
fn range_dots_do_not_join_reals() {
    assert_eq!(
        tokens("1..2.5"),
        vec![Token::Int(1), Token::DotDot, Token::Real(2.5)]
    );
    assert_eq!(
        tokens("1...5"),
        vec![Token::Int(1), Token::DotDot, Token::Real(0.5)]
    );
}

#[test]
fn brackets() {
    assert_eq!(
        tokens("a[0]"),
        vec![
            Token::Ident("a".into()),
            Token::LBracket,
            Token::Int(0),
            Token::RBracket,
        ]
    );
}

#[test]
fn compound_assignment_tokens() {
    assert_eq!(
        tokens("+= -= *= /="),
        vec![Token::PlusEq, Token::MinusEq, Token::StarEq, Token::SlashEq]
    );
    assert_eq!(
        tokens("a-=1"),
        vec![Token::Ident("a".into()), Token::MinusEq, Token::Int(1)]
    );
}

#[test]
fn ranges_next_to_numbers() {
    assert_eq!(
        tokens("0..=9"),
        vec![Token::Int(0), Token::DotDotEq, Token::Int(9)]
    );
    assert_eq!(
        tokens("[1..]"),
        vec![
            Token::LBracket,
            Token::Int(1),
            Token::DotDot,
            Token::RBracket
        ]
    );
    assert_eq!(tokens("..5"), vec![Token::DotDot, Token::Int(5)]);
}
