mod error;
mod expr;
mod token_stream;

#[cfg(test)]
mod tests;

pub use error::ParseError;

use kmpl_ast::{Expr, Span};
use kmpl_lexer::Token;

use token_stream::TokenStream;

/// Parses a single expression that must use up all tokens.
pub fn parse_expr(tokens: Vec<(Token, Span)>) -> Result<Expr, ParseError> {
    let mut ts = TokenStream::new(tokens);
    let e = expr::expr(&mut ts)?;
    if !ts.at_end() {
        return Err(ts.error_expected("end of expression"));
    }
    Ok(e)
}
