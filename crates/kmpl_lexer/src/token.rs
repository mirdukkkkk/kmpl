use logos::Logos;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip(r"[ \t\r\n]+"))]
#[logos(skip(r"//[^\n]*", allow_greedy = true))]
pub enum Token {
    #[token("const")] Const,
    #[token("fn")] Fn,
    #[token("let")] Let,
    #[token("if")] If,
    #[token("else")] Else,
    #[token("while")] While,
    #[token("import")] Import,
    #[token("struct")] Struct,
    #[token("pub")] Pub,
    #[token("enum")] Enum,
    #[token("match")] Match,
    #[token("some")] KwSome,
    #[token("none")] KwNone,
    #[token("for")] For,
    #[token("in")] In,
    #[token("break")] Break,
    #[token("continue")] Continue,
    #[token("return")] Return,
    #[token("true")] True,
    #[token("false")] False,

    #[token("+")] Plus,
    #[token("-")] Minus,
    #[token("*")] Star,
    #[token("/")] Slash,
    #[token("%")] Percent,
    #[token("!")] Bang,
    #[token("&&")] And,
    #[token("||")] Or,
    #[token("&")] Amp,
    #[token("^")] Caret,
    #[token("~")] Tilde,
    #[token("<<")] Shl,
    #[token(">>")] Shr,
    /// Bitwise or, and it separates types in a generic bound: `T: int | real`.
    #[token("|")] Pipe,
    #[token("==")] EqEq,
    #[token("!=")] NotEq,
    #[token("<")] Lt,
    #[token("<=")] Le,
    #[token(">")] Gt,
    #[token(">=")] Ge,
    #[token("=")] Assign,
    #[token("+=")] PlusEq,
    #[token("-=")] MinusEq,
    #[token("*=")] StarEq,
    #[token("/=")] SlashEq,
    #[token("%=")] PercentEq,
    #[token("&=")] AmpEq,
    #[token("|=")] PipeEq,
    #[token("^=")] CaretEq,
    #[token("<<=")] ShlEq,
    #[token(">>=")] ShrEq,
    #[token("->")] Arrow,

    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token("[")] LBracket,
    #[token("]")] RBracket,
    #[token(",")] Comma,
    #[token(";")] Semi,
    #[token(":")] Colon,
    #[token("::")] ColonColon,
    #[token("=>")] FatArrow,
    #[token("?")] Question,
    #[token(".")] Dot,
    #[token("..")] DotDot,
    #[token("..=")] DotDotEq,

    // Any characters except a line break, with `\` escaping the next one.
    #[regex(r#""([^"\\\n]|\\.)*""#, |l| unescape(l.slice()))]
    Str(String),

    #[regex("[a-zA-Z_][a-zA-Z0-9_]*", |l| l.slice().to_owned())]
    Ident(String),

    // A real is `1.5`, `.5`, `1.5e-3`, `.5E+2` or `1e6` (an exponent
    // without a dot is still a real). `i` after it makes it imaginary.
    #[regex(r"([0-9]*\.[0-9]+([eE][+-]?[0-9]+)?|[0-9]+[eE][+-]?[0-9]+)i", |l| {
        let s = l.slice();
        parse_real(&s[..s.len() - 1])
    })]
    Imag(f64),

    #[regex(r"[0-9]*\.[0-9]+([eE][+-]?[0-9]+)?|[0-9]+[eE][+-]?[0-9]+", |l| parse_real(l.slice()))]
    Real(f64),

    #[regex(r"[0-9]+", |l| l.slice().parse::<i64>().ok())]
    Int(i64),
    /// One past the largest `int`. It is only a number after a minus sign.
    #[token("9223372036854775808")]
    MinIntDigits,
}

/// `None` (a lex error) for literals too large for `f64`, like `1e400`,
/// instead of silently becoming `inf`. Too small ones round to `0.0`.
fn parse_real(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// The text of a string literal, quotes included. `None` (a lex error)
/// for an unknown escape.
fn unescape(literal: &str) -> Option<String> {
    let inner = &literal[1..literal.len() - 1];
    let mut text = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        text.push(match chars.next()? {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '"' => '"',
            '\\' => '\\',
            _ => return None,
        });
    }
    Some(text)
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Token::Const => "const",
            Token::Fn => "fn",
            Token::Let => "let",
            Token::If => "if",
            Token::Else => "else",
            Token::While => "while",
            Token::Import => "import",
            Token::Struct => "struct",
            Token::Pub => "pub",
            Token::Enum => "enum",
            Token::Match => "match",
            Token::KwSome => "some",
            Token::KwNone => "none",
            Token::For => "for",
            Token::In => "in",
            Token::Break => "break",
            Token::Continue => "continue",
            Token::Return => "return",
            Token::True => "true",
            Token::False => "false",
            Token::Plus => "+",
            Token::Minus => "-",
            Token::Star => "*",
            Token::Slash => "/",
            Token::Percent => "%",
            Token::Bang => "!",
            Token::And => "&&",
            Token::Or => "||",
            Token::Pipe => "|",
            Token::Amp => "&",
            Token::Caret => "^",
            Token::Tilde => "~",
            Token::Shl => "<<",
            Token::Shr => ">>",
            Token::EqEq => "==",
            Token::NotEq => "!=",
            Token::Lt => "<",
            Token::Le => "<=",
            Token::Gt => ">",
            Token::Ge => ">=",
            Token::Assign => "=",
            Token::PlusEq => "+=",
            Token::MinusEq => "-=",
            Token::StarEq => "*=",
            Token::SlashEq => "/=",
            Token::PercentEq => "%=",
            Token::AmpEq => "&=",
            Token::PipeEq => "|=",
            Token::CaretEq => "^=",
            Token::ShlEq => "<<=",
            Token::ShrEq => ">>=",
            Token::Arrow => "->",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::LBrace => "{",
            Token::RBrace => "}",
            Token::LBracket => "[",
            Token::RBracket => "]",
            Token::Comma => ",",
            Token::Semi => ";",
            Token::Colon => ":",
            Token::ColonColon => "::",
            Token::FatArrow => "=>",
            Token::Question => "?",
            Token::Dot => ".",
            Token::DotDot => "..",
            Token::DotDotEq => "..=",
            Token::Str(text) => return write!(f, "string {text:?}"),
            Token::Ident(name) => return write!(f, "identifier `{name}`"),
            Token::Imag(v) => return write!(f, "number `{v:?}i`"),
            Token::Real(v) => return write!(f, "number `{v:?}`"),
            Token::Int(v) => return write!(f, "number `{v}`"),
            Token::MinIntDigits => return write!(f, "number `9223372036854775808`"),
        };
        write!(f, "`{text}`")
    }
}
