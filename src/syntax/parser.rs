use crate::error::{Pos, render};
use crate::syntax::ast::*;
use crate::syntax::token::Token;
use anyhow::Result;
use std::path::PathBuf;

pub struct Parser {
    tokens: Vec<(Token, Pos)>,
    pos: usize,
    file: PathBuf,
    lines: Vec<String>,
}

impl Parser {
    pub fn new(tokens: Vec<(Token, Pos)>, file: PathBuf, lines: Vec<String>) -> Self {
        Self {
            tokens,
            pos: 0,
            file,
            lines,
        }
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .map(|t| &t.0)
            .unwrap_or(&Token::Eof)
    }

    fn pos(&self) -> Pos {
        self.tokens
            .get(self.pos)
            .map(|t| t.1)
            .or_else(|| self.tokens.last().map(|t| t.1))
            .unwrap_or(Pos::new(0, 0))
    }

    fn err(&self, msg: impl std::fmt::Display) -> anyhow::Error {
        anyhow::anyhow!(render(
            &self.file,
            self.pos(),
            &msg.to_string(),
            &self.lines
        ))
    }

    fn advance(&mut self) -> Token {
        let t = self
            .tokens
            .get(self.pos)
            .map(|t| t.0.clone())
            .unwrap_or(Token::Eof);

        self.pos += 1;

        t
    }

    fn expect(&mut self, expected: &Token) -> Result<Token> {
        let t = self.advance();

        if std::mem::discriminant(&t) != std::mem::discriminant(expected) {
            return Err(self.err(format!("expected {expected}, got {t}")));
        }

        Ok(t)
    }

    fn expect_ident(&mut self) -> Result<String> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            t => Err(self.err(format!("expected identifier, got {t}"))),
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
        let mut uses = Vec::new();
        let mut functions = Vec::new();
        let mut globals = Vec::new();

        loop {
            match self.peek() {
                Token::Use => {
                    let line = self.pos().line;
                    self.advance();
                    uses.push(self.parse_use_path(line)?);
                }

                Token::Pub => {
                    self.advance();

                    match self.peek() {
                        Token::Use => {
                            let line = self.pos().line;
                            self.advance();
                            uses.push(mark_pub(self.parse_use_path(line)?));
                        }

                        _ => self.parse_decl(true, &mut functions, &mut globals)?,
                    }
                }

                Token::Void | Token::Int | Token::Float | Token::Str => {
                    self.parse_decl(false, &mut functions, &mut globals)?;
                }

                Token::Mut => {
                    self.advance();

                    let var_type = self.parse_type()?;
                    let name = self.expect_ident()?;

                    self.expect(&Token::Equals)?;

                    let value = self.expr()?;

                    self.expect(&Token::Semi)?;

                    globals.push(GlobalVar {
                        pub_: false,
                        var_type,
                        name,
                        value,
                    });
                }

                Token::Eof => break,

                t => return Err(self.err(format!("expected declaration, got {t}"))),
            }
        }

        Ok(Program {
            uses,
            functions,
            globals,
        })
    }

    fn parse_decl(
        &mut self,
        pub_: bool,
        functions: &mut Vec<Function>,
        globals: &mut Vec<GlobalVar>,
    ) -> Result<()> {
        let line = self.pos().line;
        let var_type = self.parse_type()?;
        let name = self.expect_ident()?;

        if matches!(self.peek(), Token::LParen) {
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

            functions.push(Function {
                pub_,
                return_type: var_type,
                name,
                params,
                body,
                line,
            });
        } else {
            self.expect(&Token::Equals)?;

            let value = self.expr()?;

            self.expect(&Token::Semi)?;

            globals.push(GlobalVar {
                pub_,
                var_type,
                name,
                value,
            });
        }

        Ok(())
    }

    fn parse_use_path(&mut self, line: usize) -> Result<Use> {
        let mut path = vec![self.expect_ident()?];

        while let Token::ColonColon | Token::Colon = self.peek() {
            self.advance();

            match self.peek() {
                Token::Star => {
                    self.advance();
                    self.expect(&Token::Semi)?;

                    return Ok(Use::Wildcard {
                        pub_: false,
                        path,
                        line,
                    });
                }

                Token::LBrace => {
                    self.advance();

                    let mut names = Vec::new();

                    while !matches!(self.peek(), Token::RBrace) {
                        if !names.is_empty() {
                            self.expect(&Token::Comma)?;
                        }

                        names.push(self.expect_ident()?);
                    }

                    self.expect(&Token::RBrace)?;
                    self.expect(&Token::Semi)?;

                    return Ok(Use::Items {
                        pub_: false,
                        path,
                        names,
                        line,
                    });
                }

                _ => path.push(self.expect_ident()?),
            }
        }

        self.expect(&Token::Semi)?;

        if path.len() == 1 {
            Ok(Use::Module {
                pub_: false,
                path,
                line,
            })
        } else {
            let name = path.pop().unwrap();

            Ok(Use::Item {
                pub_: false,
                path,
                name,
                line,
            })
        }
    }

    fn parse_mut_opt(&mut self) -> bool {
        if matches!(self.peek(), Token::Mut) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn parse_param(&mut self) -> Result<Param> {
        let mut_ = self.parse_mut_opt();
        let param_type = self.parse_type()?;
        let name = self.expect_ident()?;

        Ok(Param {
            mut_,
            param_type,
            name,
        })
    }

    fn parse_type(&mut self) -> Result<FllufType> {
        match self.advance() {
            Token::Int => Ok(FllufType::Int),
            Token::Float => Ok(FllufType::Float),
            Token::Void => Ok(FllufType::Void),
            Token::Str => Ok(FllufType::String),

            t => Err(self.err(format!("expected type (int/float/void/string), got {t}"))),
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
                let line = self.pos().line;
                self.advance();

                let value = self.expr()?;

                self.expect(&Token::Semi)?;

                Ok(Stmt::Return(value, line))
            }

            Token::If => self.parse_if(),

            Token::Mut | Token::Int | Token::Float | Token::Void | Token::Str => {
                let line = self.pos().line;
                let mut_ = self.parse_mut_opt();
                let var_type = self.parse_type()?;
                let name = self.expect_ident()?;

                self.expect(&Token::Equals)?;

                let value = self.expr()?;

                self.expect(&Token::Semi)?;

                Ok(Stmt::VarDecl {
                    mut_,
                    var_type,
                    name,
                    value,
                    line,
                })
            }

            _ => {
                let line = self.pos().line;
                let e = self.expr()?;

                if matches!(self.peek(), Token::Equals) {
                    match e {
                        Expr::Variable(name, _) => {
                            self.advance();

                            let value = self.expr()?;

                            self.expect(&Token::Semi)?;

                            Ok(Stmt::Assign { name, value, line })
                        }

                        _ => Err(self.err("left-hand side of assignment must be a variable")),
                    }
                } else {
                    self.expect(&Token::Semi)?;
                    Ok(Stmt::Expr(e, line))
                }
            }
        }
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
                Token::Eq => BinOp::Eq,
                Token::Ne => BinOp::Ne,
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::Le => BinOp::Le,
                Token::Ge => BinOp::Ge,

                _ => break,
            };

            let line = self.pos().line;

            self.advance();

            let right = self.parse_add()?;

            left = Expr::BinaryOp(Box::new(left), op, Box::new(right), line);
        }

        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr> {
        let mut left = self.parse_mul()?;

        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,

                _ => break,
            };

            let line = self.pos().line;

            self.advance();

            let right = self.parse_mul()?;

            left = Expr::BinaryOp(Box::new(left), op, Box::new(right), line);
        }

        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr> {
        let mut left = self.parse_atom()?;

        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,

                _ => break,
            };

            let line = self.pos().line;
            self.advance();

            let right = self.parse_atom()?;

            left = Expr::BinaryOp(Box::new(left), op, Box::new(right), line);
        }

        Ok(left)
    }

    fn parse_atom(&mut self) -> Result<Expr> {
        let line = self.pos().line;

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

            Token::Ident(_) => {
                let name = self.expect_ident()?;

                if matches!(self.peek(), Token::LParen) {
                    let args = self.args()?;
                    Ok(Expr::Call(name, args, line))
                } else if matches!(self.peek(), Token::ColonColon) {
                    self.advance();

                    let second = match self.advance() {
                        Token::Ident(s) => s,
                        t => {
                            return Err(self.err(format!("expected identifier after ::, got {t}")));
                        }
                    };

                    if matches!(self.peek(), Token::LParen) {
                        let args = self.args()?;
                        Ok(Expr::ModuleCall(name, second, args, line))
                    } else {
                        Ok(Expr::ModuleVar(name, second, line))
                    }
                } else {
                    Ok(Expr::Variable(name, line))
                }
            }

            t => Err(self.err(format!("unexpected {t}"))),
        }
    }
}

fn mark_pub(u: Use) -> Use {
    match u {
        Use::Module { path, line, .. } => Use::Module {
            pub_: true,
            path,
            line,
        },
        Use::Wildcard { path, line, .. } => Use::Wildcard {
            pub_: true,
            path,
            line,
        },
        Use::Item {
            path, name, line, ..
        } => Use::Item {
            pub_: true,
            path,
            name,
            line,
        },
        Use::Items {
            path, names, line, ..
        } => Use::Items {
            pub_: true,
            path,
            names,
            line,
        },
    }
}
