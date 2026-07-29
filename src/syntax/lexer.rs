use crate::syntax::token::Token;

#[derive(Debug)]
pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();

        self.pos += 1;

        c
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_number(&mut self, first: char) -> Token {
        let mut s = String::new();

        s.push(first);

        let mut is_float = false;

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else if c == '.' && !is_float {
                is_float = true;

                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if is_float {
            Token::FloatLit(s.parse().unwrap_or(0.0))
        } else {
            Token::Integer(s.parse().unwrap_or(0))
        }
    }

    fn read_ident(&mut self, first: char) -> Token {
        let mut s = String::new();

        s.push(first);

        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }

        match s.as_str() {
            "use" => Token::Use,
            "return" => Token::Return,
            "void" => Token::Void,
            "int" => Token::Int,
            "float" => Token::Float,
            "string" => Token::Str,
            "if" => Token::If,
            "else" => Token::Else,

            _ => Token::Ident(s),
        }
    }

    fn read_string(&mut self) -> Token {
        let mut s = String::new();

        while let Some(c) = self.advance() {
            if c == '"' {
                break;
            }

            if c == '\\' {
                if let Some(next) = self.advance() {
                    match next {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),

                        _ => {
                            s.push('\\');
                            s.push(next);
                        }
                    }
                }
            } else {
                s.push(c);
            }
        }

        Token::StringLit(s)
    }

    fn skip_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }
}

impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.skip_whitespace();

            let c = self.advance()?;

            return Some(match c {
                '+' => Token::Plus,
                '-' => Token::Minus,
                '*' => Token::Star,
                '/' => {
                    if self.peek() == Some('/') {
                        self.advance();
                        self.skip_comment();
                        continue;
                    } else {
                        Token::Slash
                    }
                }
                '(' => Token::LParen,
                ')' => Token::RParen,
                '{' => Token::LBrace,
                '}' => Token::RBrace,
                ':' => {
                    if self.peek() == Some(':') {
                        self.advance();
                        Token::ColonColon
                    } else {
                        Token::Colon
                    }
                }
                ';' => Token::Semi,
                '=' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Eq
                    } else {
                        Token::Equals
                    }
                }
                ',' => Token::Comma,
                '!' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Ne
                    } else {
                        panic!("unexpected '!'")
                    }
                }
                '<' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Le
                    } else {
                        Token::Lt
                    }
                }
                '>' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Ge
                    } else {
                        Token::Gt
                    }
                }
                '"' => self.read_string(),

                c if c.is_ascii_digit() => self.read_number(c),
                c if c.is_alphabetic() || c == '_' => self.read_ident(c),

                c => panic!("unexpected character: '{c}'"),
            });
        }
    }
}
