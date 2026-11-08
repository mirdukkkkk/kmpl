use kmpl_ast::{Ident, Span};
use kmpl_lexer::Token;

use crate::error::{PResult, ParseError};

pub struct TokenStream {
    tokens: Vec<(Token, Span)>,
    pos: usize,
    /// Empty span right after the last token, used for "found end of file".
    eof: Span,
    /// Off in the header of `if`, `while` and `for`, where a `{` after a
    /// name starts the body. Back on inside any brackets.
    pub struct_literals: bool,
}

impl TokenStream {
    pub fn new(tokens: Vec<(Token, Span)>) -> Self {
        let end = tokens.last().map_or(0, |(_, span)| span.end);

        Self {
            tokens,
            pos: 0,
            eof: Span::new(end, end),
            struct_literals: true,
        }
    }

    pub fn peek(&self) -> Option<&Token> {
        self.peek_nth(0)
    }

    /// `peek_nth(0)` is the same as `peek()`.
    pub fn peek_nth(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.pos + n).map(|(tok, _)| tok)
    }

    /// Span of the next token, or the end-of-file span.
    pub fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map_or(self.eof, |(_, span)| *span)
    }

    pub fn next(&mut self) -> Option<(Token, Span)> {
        let item = self.tokens.get(self.pos).cloned();
        if item.is_some() {
            self.pos += 1;
        }
        item
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Consumes the next token if it equals `tok`.
    pub fn eat(&mut self, tok: &Token) -> Option<Span> {
        if self.peek() == Some(tok) {
            self.next().map(|(_, span)| span)
        } else {
            None
        }
    }

    pub fn expect(&mut self, tok: Token) -> PResult<Span> {
        match self.eat(&tok) {
            Some(span) => Ok(span),
            None => Err(self.error_expected(&tok.to_string())),
        }
    }

    pub fn expect_ident(&mut self) -> PResult<Ident> {
        if let Some(Token::Ident(_)) = self.peek()
            && let Some((Token::Ident(name), span)) = self.next()
        {
            return Ok(Ident { name, span });
        }
        Err(self.error_expected("identifier"))
    }

    /// "expected {what}, found <next token>".
    pub fn error_expected(&self, what: &str) -> ParseError {
        ParseError::expected(what, self.peek(), self.peek_span())
    }
}
