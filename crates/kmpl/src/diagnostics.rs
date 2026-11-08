//! Error messages with file, line, column and the source line underlined.
//! Every stage (lexer, parser, checker, interpreter) reports through here.
//!
//! ```text
//! error: division by zero
//!  --> examples/x.kmpl:3:11
//!   |
//! 3 |     print(1 / z);
//!   |           ^^^^^
//! ```

#[cfg(test)]
mod tests;

use kmpl_ast::Span;

/// Formats one error. `span` is in bytes of `src`; line and column are
/// 1-based, and the column counts characters, not bytes.
pub fn render(path: &str, src: &str, message: &str, span: Span) -> String {
    let start = floor_char_boundary(src, span.start);
    let end = floor_char_boundary(src, span.end.max(start));

    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[start..].find('\n').map_or(src.len(), |i| start + i);
    let line = src[line_start..line_end].trim_end_matches('\r');
    let line_no = src[..start].matches('\n').count() + 1;
    let col = src[line_start..start].chars().count() + 1;

    // Copy tabs from the line so the carets line up with the code.
    let indent: String = src[line_start..start]
        .chars()
        .map(|c| if c == '\t' { '\t' } else { ' ' })
        .collect();
    // A span over several lines is underlined to the end of its first line;
    // an empty span (end of file) still gets one caret.
    let width = src[start..end.min(line_end)].chars().count().max(1);
    let carets = "^".repeat(width);

    let num = line_no.to_string();
    let pad = " ".repeat(num.len());
    format!(
        "error: {message}\n\
         {pad}--> {path}:{line_no}:{col}\n\
         {pad} |\n\
         {num} | {line}\n\
         {pad} | {indent}{carets}\n"
    )
}

/// Largest index `<= i` that starts a character, clamped to the length.
fn floor_char_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Message for a lexer error at `span`: a too-large number or a character
/// that starts no token. A number that failed to lex was out of range,
/// since the lexer accepts every other digit sequence.
pub fn lex_error_message(src: &str, span: Span) -> String {
    let start = floor_char_boundary(src, span.start);
    let end = floor_char_boundary(src, span.end.max(start));
    let text = &src[start..end];

    let starts_like_number = text.starts_with(|c: char| c.is_ascii_digit() || c == '.');
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        format!("integer literal `{text}` is too large for `int`")
    } else if starts_like_number && text.len() > 1 {
        format!("real literal `{text}` is out of range")
    } else {
        let shown = text.chars().next().unwrap_or(' ');
        format!("unexpected character `{shown}`")
    }
}
