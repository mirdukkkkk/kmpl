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
fn single_ampersand_and_pipe_are_errors() {
    assert_eq!(lex("a & b"), Err(Span::new(2, 3)));
    assert_eq!(lex("a | b"), Err(Span::new(2, 3)));
}

#[test]
fn numbers_are_64_bit() {
    assert_eq!(
        tokens("9000000000 3.141592653589793"),
        vec![Token::Int(9_000_000_000), Token::Real(std::f64::consts::PI)]
    );
}
