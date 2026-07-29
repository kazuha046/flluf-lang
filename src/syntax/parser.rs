use crate::syntax::ast::*;
use crate::syntax::token::Token;
use anyhow::{Result, bail};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);

        self.pos += 1;

        t
    }

    fn expect(&mut self, expected: &Token) -> Result<Token> {
        let t = self.advance();

        if std::mem::discriminant(&t) != std::mem::discriminant(expected) {
            bail!("expected {expected}, got {t}");
        }

        Ok(t)
    }

    fn expect_ident(&mut self) -> Result<String> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            t => bail!("expected identifier, got {t}"),
        }
    }

    fn skip_semi(&mut self) {
        if matches!(self.peek(), Token::Semi) {
            self.advance();
        }
    }

    fn args(&mut self) -> Result<Vec<Expr>> {
        self.expect(&Token::LParen)?;

        let mut args = Vec::new();

        while !matches!(self.peek(), Token::RParen) {
            if !args.is_empty() {
                self.expect(&Token::Comma)?;
            }

            args.push(self.expr()?);
        }

        self.expect(&Token::RParen)?;

        Ok(args)
    }

    pub fn parse(&mut self) -> Result<Program> {
        let mut use_system = false;
        let mut functions = Vec::new();

        loop {
            match self.peek() {
                Token::Use => {
                    self.advance();

                    self.expect(&Token::Ident("System".to_string()))?;
                    self.expect(&Token::Semi)?;

                    use_system = true;
                }

                Token::Void | Token::Int | Token::Float | Token::Str => {
                    functions.push(self.parse_function()?);
                }

                Token::Eof => break,

                t => bail!("expected function declaration, got {t}"),
            }
        }

        Ok(Program {
            use_system,
            functions,
        })
    }

    fn parse_function(&mut self) -> Result<Function> {
        let return_type = self.parse_type()?;
        let name = self.expect_ident()?;

        self.expect(&Token::LParen)?;

        let mut params = Vec::new();

        while !matches!(self.peek(), Token::RParen) {
            if !params.is_empty() {
                self.expect(&Token::Comma)?;
            }

            params.push(self.parse_param()?);
        }

        self.expect(&Token::RParen)?;
        self.expect(&Token::Colon)?;

        let body = self.parse_block()?;

        Ok(Function {
            return_type,
            name,
            params,
            body,
        })
    }

    fn parse_param(&mut self) -> Result<Param> {
        let param_type = self.parse_type()?;
        let name = self.expect_ident()?;

        Ok(Param { param_type, name })
    }

    fn parse_type(&mut self) -> Result<FllufType> {
        match self.advance() {
            Token::Int => Ok(FllufType::Int),
            Token::Float => Ok(FllufType::Float),
            Token::Void => Ok(FllufType::Void),
            Token::Str => Ok(FllufType::String),

            t => bail!("expected type (int/float/void/string), got {t}"),
        }
    }

    fn parse_block(&mut self) -> Result<Block> {
        self.expect(&Token::LBrace)?;

        let mut statements = Vec::new();

        while !matches!(self.peek(), Token::RBrace) {
            statements.push(self.parse_stmt()?);
        }

        self.expect(&Token::RBrace)?;

        Ok(Block { statements })
    }

    fn parse_stmt(&mut self) -> Result<Stmt> {
        match self.peek() {
            Token::Return => {
                self.advance();

                let value = self.expr()?;

                self.skip_semi();

                Ok(Stmt::Return(value))
            }

            Token::If => self.parse_if(),

            Token::Int | Token::Float | Token::Void | Token::Str => self.parse_var_decl(),

            _ => {
                let value = self.expr()?;

                self.skip_semi();

                Ok(Stmt::Expr(value))
            }
        }
    }

    fn parse_var_decl(&mut self) -> Result<Stmt> {
        let var_type = self.parse_type()?;
        let name = self.expect_ident()?;

        self.expect(&Token::Equals)?;

        let value = self.expr()?;

        self.skip_semi();

        Ok(Stmt::VarDecl {
            var_type,
            name,
            value,
        })
    }

    fn parse_if(&mut self) -> Result<Stmt> {
        self.advance();

        self.expect(&Token::LParen)?;

        let cond = self.expr()?;

        self.expect(&Token::RParen)?;
        self.expect(&Token::Colon)?;

        let then_block = self.parse_block()?;

        let mut else_ifs = Vec::new();
        let mut else_block = None;

        while matches!(self.peek(), Token::Else) {
            self.advance();

            if matches!(self.peek(), Token::If) {
                self.advance();

                self.expect(&Token::LParen)?;

                let cond = self.expr()?;

                self.expect(&Token::RParen)?;
                self.expect(&Token::Colon)?;

                else_ifs.push((cond, self.parse_block()?));
            } else {
                self.expect(&Token::Colon)?;

                else_block = Some(self.parse_block()?);

                break;
            }
        }

        Ok(Stmt::If {
            cond,
            then_block,
            else_ifs,
            else_block,
        })
    }

    fn expr(&mut self) -> Result<Expr> {
        self.parse_cmp()
    }

    fn parse_cmp(&mut self) -> Result<Expr> {
        let mut left = self.parse_add()?;

        loop {
            let op = match self.peek() {
                Token::Eq => {
                    self.advance();
                    BinOp::Eq
                }
                Token::Ne => {
                    self.advance();
                    BinOp::Ne
                }
                Token::Lt => {
                    self.advance();
                    BinOp::Lt
                }
                Token::Gt => {
                    self.advance();
                    BinOp::Gt
                }
                Token::Le => {
                    self.advance();
                    BinOp::Le
                }
                Token::Ge => {
                    self.advance();
                    BinOp::Ge
                }

                _ => break,
            };

            left = Expr::BinaryOp(Box::new(left), op, Box::new(self.parse_add()?));
        }

        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr> {
        let mut left = self.parse_mul()?;

        loop {
            let op = match self.peek() {
                Token::Plus => {
                    self.advance();
                    BinOp::Add
                }
                Token::Minus => {
                    self.advance();
                    BinOp::Sub
                }

                _ => break,
            };

            left = Expr::BinaryOp(Box::new(left), op, Box::new(self.parse_mul()?));
        }

        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr> {
        let mut left = self.parse_atom()?;

        loop {
            let op = match self.peek() {
                Token::Star => {
                    self.advance();
                    BinOp::Mul
                }
                Token::Slash => {
                    self.advance();
                    BinOp::Div
                }

                _ => break,
            };

            left = Expr::BinaryOp(Box::new(left), op, Box::new(self.parse_atom()?));
        }

        Ok(left)
    }

    fn parse_atom(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Token::Integer(n) => {
                self.advance();
                Ok(Expr::IntLit(n))
            }
            Token::FloatLit(n) => {
                self.advance();
                Ok(Expr::FloatLit(n))
            }
            Token::StringLit(s) => {
                self.advance();
                Ok(Expr::StringLit(s))
            }

            Token::LParen => {
                self.advance();

                let e = self.expr()?;

                self.expect(&Token::RParen)?;

                Ok(e)
            }

            Token::Ident(name) => {
                self.advance();

                if matches!(self.peek(), Token::LParen) {
                    let args = self.args()?;

                    Ok(Expr::Call(name, args))
                } else if matches!(self.peek(), Token::ColonColon) {
                    self.advance();

                    let func = self.ident_or_keyword();
                    let args = self.args()?;

                    Ok(Expr::ModuleCall(name, func, args))
                } else {
                    Ok(Expr::Variable(name))
                }
            }

            t => bail!("unexpected {t}"),
        }
    }

    fn ident_or_keyword(&mut self) -> String {
        match self.advance() {
            Token::Ident(s) => s,
            Token::If => "if".to_string(),
            Token::Else => "else".to_string(),
            Token::Return => "return".to_string(),
            Token::Use => "use".to_string(),
            Token::Void => "void".to_string(),
            Token::Int => "int".to_string(),
            Token::Float => "float".to_string(),
            Token::Str => "string".to_string(),

            t => panic!("expected name, got {t}"),
        }
    }
}
