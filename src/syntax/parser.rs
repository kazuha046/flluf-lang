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
        let mut uses = Vec::new();
        let mut functions = Vec::new();
        let mut globals = Vec::new();

        loop {
            match self.peek() {
                Token::Use => {
                    self.advance();
                    let u = self.parse_use_path()?;

                    match u {
                        Use::System => use_system = true,
                        other => uses.push(other),
                    }
                }

                Token::Pub => {
                    self.advance();

                    match self.peek() {
                        Token::Use => {
                            self.advance();

                            match mark_pub(self.parse_use_path()?) {
                                Use::System => use_system = true,
                                other => uses.push(other),
                            }
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

                    self.skip_semi();

                    globals.push(GlobalVar {
                        pub_: false,
                        var_type,
                        name,
                        value,
                    });
                }

                Token::Eof => break,

                t => bail!("expected declaration, got {t}"),
            }
        }

        Ok(Program {
            use_system,
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
            });
        } else {
            self.expect(&Token::Equals)?;

            let value = self.expr()?;

            self.skip_semi();
            globals.push(GlobalVar {
                pub_,
                var_type,
                name,
                value,
            });
        }

        Ok(())
    }

    fn parse_use_path(&mut self) -> Result<Use> {
        let mut path = vec![self.expect_ident()?];

        while let Token::ColonColon | Token::Colon = self.peek() {
            self.advance();

            match self.peek() {
                Token::Star => {
                    self.advance();
                    self.expect(&Token::Semi)?;

                    return Ok(Use::Wildcard { pub_: false, path });
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
                    });
                }

                _ => path.push(self.expect_ident()?),
            }
        }

        self.expect(&Token::Semi)?;

        if path.len() == 1 && path[0] == "System" {
            Ok(Use::System)
        } else if path.len() == 1 {
            Ok(Use::Module { pub_: false, path })
        } else {
            let name = path.pop().unwrap();

            Ok(Use::Item {
                pub_: false,
                path,
                name,
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

            Token::Mut | Token::Int | Token::Float | Token::Void | Token::Str => {
                let mut_ = self.parse_mut_opt();
                let var_type = self.parse_type()?;
                let name = self.expect_ident()?;

                self.expect(&Token::Equals)?;

                let value = self.expr()?;

                self.skip_semi();

                Ok(Stmt::VarDecl {
                    mut_,
                    var_type,
                    name,
                    value,
                })
            }

            _ => {
                let e = self.expr()?;

                if matches!(self.peek(), Token::Equals) {
                    match e {
                        Expr::Variable(name) => {
                            self.advance();

                            let value = self.expr()?;

                            self.skip_semi();

                            Ok(Stmt::Assign { name, value })
                        }

                        _ => bail!("left-hand side of assignment must be a variable"),
                    }
                } else {
                    self.skip_semi();
                    Ok(Stmt::Expr(e))
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

            Token::Ident(_) => {
                let name = self.expect_ident()?;

                if matches!(self.peek(), Token::LParen) {
                    let args = self.args()?;
                    Ok(Expr::Call(name, args))
                } else if matches!(self.peek(), Token::ColonColon) {
                    self.advance();

                    let second = match self.advance() {
                        Token::Ident(s) => s,
                        t => bail!("expected identifier after ::, got {t}"),
                    };

                    if matches!(self.peek(), Token::LParen) {
                        let args = self.args()?;
                        Ok(Expr::ModuleCall(name, second, args))
                    } else {
                        Ok(Expr::ModuleVar(name, second))
                    }
                } else {
                    Ok(Expr::Variable(name))
                }
            }

            t => bail!("unexpected {t}"),
        }
    }
}

fn mark_pub(u: Use) -> Use {
    match u {
        Use::Module { path, .. } => Use::Module { pub_: true, path },
        Use::Wildcard { path, .. } => Use::Wildcard { pub_: true, path },
        Use::Item { path, name, .. } => Use::Item {
            pub_: true,
            path,
            name,
        },
        Use::Items { path, names, .. } => Use::Items {
            pub_: true,
            path,
            names,
        },

        u => u,
    }
}
