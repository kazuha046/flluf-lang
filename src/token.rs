#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Use,
    Return,
    Void,
    Int,
    Float,
    If,
    Else,
    Exit,

    Integer(i64),
    FloatLit(f64),
    StringLit(String),
    Ident(String),

    Plus,
    Minus,
    Star,
    Slash,

    Eq, // ==
    Ne, // !=
    Lt, // <
    Gt, // >
    Le, // <=
    Ge, // >=

    LParen,
    RParen,
    LBrace,
    RBrace,
    Colon,
    Semi,
    Equals,
    Comma,
    ColonColon,

    Comment,
    Eof,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Use => write!(f, "use"),
            Token::Return => write!(f, "return"),
            Token::Void => write!(f, "void"),
            Token::Int => write!(f, "int"),
            Token::Float => write!(f, "float"),
            Token::If => write!(f, "if"),
            Token::Else => write!(f, "else"),
            Token::Exit => write!(f, "Exit"),
            Token::Integer(n) => write!(f, "{n}"),
            Token::FloatLit(n) => write!(f, "{n}"),
            Token::StringLit(s) => write!(f, "\"{s}\""),
            Token::Ident(s) => write!(f, "{s}"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Eq => write!(f, "=="),
            Token::Ne => write!(f, "!="),
            Token::Lt => write!(f, "<"),
            Token::Gt => write!(f, ">"),
            Token::Le => write!(f, "<="),
            Token::Ge => write!(f, ">="),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::Colon => write!(f, ":"),
            Token::Semi => write!(f, ";"),
            Token::Equals => write!(f, "="),
            Token::Comma => write!(f, ","),
            Token::ColonColon => write!(f, "::"),
            Token::Comment => write!(f, "//"),
            Token::Eof => write!(f, "<eof>"),
        }
    }
}
