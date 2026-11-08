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
    #[token("break")] Break,
    #[token("continue")] Continue,
    #[token("return")] Return,
    #[token("true")] True,
    #[token("false")] False,

    #[token("+")] Plus,
    #[token("-")] Minus,
    #[token("*")] Star,
    #[token("/")] Slash,
    #[token("!")] Bang,
    #[token("&&")] And,
    #[token("||")] Or,
    #[token("==")] EqEq,
    #[token("!=")] NotEq,
    #[token("<")] Lt,
    #[token("<=")] Le,
    #[token(">")] Gt,
    #[token(">=")] Ge,
    #[token("=")] Assign,
    #[token("->")] Arrow,

    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token(",")] Comma,
    #[token(";")] Semi,
    #[token(":")] Colon,

    #[regex("[a-zA-Z_][a-zA-Z0-9_]*", |l| l.slice().to_owned())]
    Ident(String),

    #[regex(r"[0-9]+\.[0-9]+i", |l| {
        let s = l.slice();
        s[..s.len() - 1].parse::<f32>().ok()
    })]
    Imag(f32),

    #[regex(r"[0-9]+\.[0-9]+", |l| l.slice().parse::<f32>().ok())]
    Real(f32),

    #[regex(r"[0-9]+", |l| l.slice().parse::<i32>().ok())]
    Int(i32),
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
            Token::Break => "break",
            Token::Continue => "continue",
            Token::Return => "return",
            Token::True => "true",
            Token::False => "false",
            Token::Plus => "+",
            Token::Minus => "-",
            Token::Star => "*",
            Token::Slash => "/",
            Token::Bang => "!",
            Token::And => "&&",
            Token::Or => "||",
            Token::EqEq => "==",
            Token::NotEq => "!=",
            Token::Lt => "<",
            Token::Le => "<=",
            Token::Gt => ">",
            Token::Ge => ">=",
            Token::Assign => "=",
            Token::Arrow => "->",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::LBrace => "{",
            Token::RBrace => "}",
            Token::Comma => ",",
            Token::Semi => ";",
            Token::Colon => ":",
            Token::Ident(name) => return write!(f, "identifier `{name}`"),
            Token::Imag(v) => return write!(f, "number `{v}i`"),
            Token::Real(v) => return write!(f, "number `{v}`"),
            Token::Int(v) => return write!(f, "number `{v}`"),
        };
        write!(f, "`{text}`")
    }
}
