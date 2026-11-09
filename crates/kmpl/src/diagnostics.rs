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

/// Every source file of the program. A `Span` is only two numbers, so each
/// file gets its own range of them, one after another; which file a span
/// belongs to follows from where it starts.
#[derive(Default)]
pub struct Sources {
    files: Vec<SourceFile>,
}

struct SourceFile {
    path: String,
    text: String,
    start: usize,
}

impl Sources {
    /// Adds a file and returns the position of its first byte. The first
    /// file starts at 0. The `+ 1` keeps an empty span at the end of a file
    /// from looking like the start of the next one.
    pub fn add(&mut self, path: &str, text: &str) -> usize {
        let start = self
            .files
            .last()
            .map_or(0, |file| file.start + file.text.len() + 1);
        self.files.push(SourceFile {
            path: path.to_owned(),
            text: text.to_owned(),
            start,
        });
        start
    }

    /// Formats one error, with `span` in positions given out by `add`.
    pub fn render(&self, message: &str, span: Span) -> String {
        format!("error: {message}\n{}", self.location(span))
    }

    /// The part of an error under its first line.
    pub fn location(&self, span: Span) -> String {
        let file = self
            .files
            .iter()
            .rev()
            .find(|file| file.start <= span.start)
            .expect("a span comes from a file");
        let local = Span::new(span.start - file.start, span.end.saturating_sub(file.start));
        location(&file.path, &file.text, local)
    }
}

/// Formats one error. `span` is in bytes of `src`; line and column are
/// 1-based, and the column counts characters, not bytes.
pub fn render(path: &str, src: &str, message: &str, span: Span) -> String {
    format!("error: {message}\n{}", location(path, src, span))
}

/// The lines of an error that point into the source.
fn location(path: &str, src: &str, span: Span) -> String {
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
        "{pad}--> {path}:{line_no}:{col}\n\
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

/// Message for a lexer error at `span`: a too-large number, a bad string or
/// a character that starts no token. A number that failed to lex was out of
/// range, since the lexer accepts every other digit sequence.
pub fn lex_error_message(src: &str, span: Span) -> String {
    let start = floor_char_boundary(src, span.start);
    let end = floor_char_boundary(src, span.end.max(start));
    let text = &src[start..end];

    if text.starts_with('"') {
        return string_error_message(&src[start..]);
    }

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

/// `rest` starts at the opening quote of a string that failed to lex. If
/// the closing quote is on the same line, an escape was the problem.
fn string_error_message(rest: &str) -> String {
    let mut chars = rest.chars().skip(1);
    while let Some(c) = chars.next() {
        match c {
            '"' => return r#"unknown escape in string, use `\n`, `\t`, `\"` or `\\`"#.to_owned(),
            '\n' => break,
            '\\' => {
                chars.next();
            }
            _ => {}
        }
    }
    "unterminated string".to_owned()
}
