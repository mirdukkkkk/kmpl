use kmpl_ast::Span;
use kmpl_lexer::Token;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl ParseError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    /// "expected `;`, found `}`" or "expected expression, found end of file".
    pub fn expected(what: &str, found: Option<&Token>, span: Span) -> Self {
        let found = match found {
            Some(tok) => tok.to_string(),
            None => "end of file".to_owned(),
        };

        Self::new(format!("expected {what}, found {found}"), span)
    }
}

pub type PResult<T> = Result<T, ParseError>;
