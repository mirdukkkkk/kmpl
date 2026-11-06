mod token;

#[cfg(test)]
mod tests;

pub use token::Token;

use kmpl_ast::Span;
use logos::Logos;

pub fn lex(src: &str) -> Result<Vec<(Token, Span)>, Span> {
    let mut out = Vec::new();

    for (tok, range) in Token::lexer(src).spanned() {
        let span = Span::from(range);

        match tok {
            Ok(t) => out.push((t, span)),
            Err(_) => return Err(span),
        }
    }

    Ok(out)
}
