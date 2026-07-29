use crate::ast::{BinOp, Block, Expr, FllufType as Type, Function, Param, Program, Stmt};
use crate::token::Token;

#[derive(Debug)]
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
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);

        self.pos += 1;

        tok
    }

    fn skip_comments(&mut self) {
        while *self.peek() == Token::Comment {
            self.advance();
        }
    }

    fn expect(&mut self, expected: &Token) -> Token {
        let tok = self.advance();

        if std::mem::discriminant(&tok) != std::mem::discriminant(expected) {
            panic!("expected {expected}, got {tok}");
        }

        tok
    }

    fn skip_semi(&mut self) {
        self.skip_comments();

        if *self.peek() == Token::Semi {
            self.advance();
        }
    }

    pub fn parse_program(&mut self) -> Program {
        let mut use_system = false;
        let mut functions = Vec::new();

        while *self.peek() != Token::Eof {
            self.skip_comments();

            if *self.peek() == Token::Eof {
                break;
            }

            if *self.peek() == Token::Use {
                self.advance();
                self.expect(&Token::Ident("System".to_string()));
                self.expect(&Token::Semi);

                use_system = true;

                continue;
            }

            functions.push(self.parse_function());
        }

        Program {
            use_system,
            functions,
        }
    }

    fn parse_function(&mut self) -> Function {
        self.skip_comments();

        let return_type = self.parse_type();

        let name = match self.advance() {
            Token::Ident(s) => s,
            t => panic!("expected function name, got {t}"),
        };

        self.expect(&Token::LParen);

        let mut params = Vec::new();

        while *self.peek() != Token::RParen {
            if !params.is_empty() {
                self.expect(&Token::Comma);
            }

            params.push(self.parse_param());
        }

        self.expect(&Token::RParen);
        self.expect(&Token::Colon);

        let body = self.parse_block();

        Function {
            return_type,
            name,
            params,
            body,
        }
    }

    fn parse_param(&mut self) -> Param {
        let param_type = self.parse_type();

        let name = match self.advance() {
            Token::Ident(s) => s,
            t => panic!("expected parameter name, got {t}"),
        };

        Param { param_type, name }
    }

    fn parse_type(&mut self) -> Type {
        match self.advance() {
            Token::Int => Type::Int,
            Token::Float => Type::Float,
            Token::Void => Type::Void,

            t => panic!("expected type (int/float/void), got {t}"),
        }
    }

    fn parse_block(&mut self) -> Block {
        self.skip_comments();
        self.expect(&Token::LBrace);

        let mut statements = Vec::new();

        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            self.skip_comments();

            if *self.peek() == Token::RBrace || *self.peek() == Token::Eof {
                break;
            }

            statements.push(self.parse_stmt());
        }

        self.skip_comments();
        self.expect(&Token::RBrace);

        Block { statements }
    }

    fn parse_stmt(&mut self) -> Stmt {
        self.skip_comments();

        if *self.peek() == Token::Return {
            self.advance();

            let expr = self.parse_expr();

            self.skip_semi();

            return Stmt::Return(expr);
        }

        if *self.peek() == Token::If {
            return self.parse_if();
        }

        if self.is_type() {
            return self.parse_var_decl();
        }

        let expr = self.parse_expr();

        self.skip_semi();

        Stmt::Expr(expr)
    }

    fn is_type(&self) -> bool {
        matches!(self.peek(), Token::Int | Token::Float | Token::Void)
    }

    fn parse_var_decl(&mut self) -> Stmt {
        let var_type = self.parse_type();

        let name = match self.advance() {
            Token::Ident(s) => s,
            t => panic!("expected variable name, got {t}"),
        };

        self.expect(&Token::Equals);

        let value = self.parse_expr();

        self.skip_semi();

        Stmt::VarDecl {
            var_type,
            name,
            value,
        }
    }

    fn parse_if(&mut self) -> Stmt {
        self.advance();
        self.expect(&Token::LParen);

        let cond = self.parse_expr();

        self.expect(&Token::RParen);
        self.expect(&Token::Colon);

        let then_block = self.parse_block();

        let mut else_ifs = Vec::new();
        let mut else_block = None;

        self.skip_comments();

        while *self.peek() == Token::Else {
            self.advance();

            if *self.peek() == Token::If {
                self.advance();
                self.expect(&Token::LParen);

                let cond = self.parse_expr();

                self.expect(&Token::RParen);
                self.expect(&Token::Colon);

                let block = self.parse_block();

                else_ifs.push((cond, block));
            } else {
                self.expect(&Token::Colon);

                else_block = Some(self.parse_block());

                break;
            }
        }

        Stmt::If {
            cond,
            then_block,
            else_ifs,
            else_block,
        }
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut left = self.parse_and();

        while *self.peek() == Token::Eq
            || *self.peek() == Token::Ne
            || *self.peek() == Token::Lt
            || *self.peek() == Token::Gt
            || *self.peek() == Token::Le
            || *self.peek() == Token::Ge
        {
            let op = match self.advance() {
                Token::Eq => BinOp::Eq,
                Token::Ne => BinOp::Ne,
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::Le => BinOp::Le,
                Token::Ge => BinOp::Ge,

                _ => unreachable!(),
            };

            let right = self.parse_and();

            left = Expr::BinaryOp(Box::new(left), op, Box::new(right));
        }

        left
    }

    fn parse_and(&mut self) -> Expr {
        let mut left = self.parse_add();

        loop {
            match self.peek() {
                Token::Plus => {
                    self.advance();

                    let right = self.parse_add();

                    left = Expr::BinaryOp(Box::new(left), BinOp::Add, Box::new(right));
                }
                Token::Minus => {
                    self.advance();

                    let right = self.parse_add();

                    left = Expr::BinaryOp(Box::new(left), BinOp::Sub, Box::new(right));
                }

                _ => break,
            }
        }

        left
    }

    fn parse_add(&mut self) -> Expr {
        let mut left = self.parse_mul();

        loop {
            match self.peek() {
                Token::Star => {
                    self.advance();

                    let right = self.parse_mul();

                    left = Expr::BinaryOp(Box::new(left), BinOp::Mul, Box::new(right));
                }
                Token::Slash => {
                    self.advance();

                    let right = self.parse_mul();

                    left = Expr::BinaryOp(Box::new(left), BinOp::Div, Box::new(right));
                }

                _ => break,
            }
        }

        left
    }

    fn parse_mul(&mut self) -> Expr {
        self.parse_atom()
    }

    fn parse_name_or_keyword(&mut self) -> String {
        match self.advance() {
            Token::Ident(s) => s,
            Token::Exit => "Exit".to_string(),
            Token::If => "If".to_string(),
            Token::Return => "Return".to_string(),
            Token::Use => "Use".to_string(),
            Token::Void => "Void".to_string(),
            Token::Int => "Int".to_string(),
            Token::Float => "Float".to_string(),
            Token::Else => "Else".to_string(),

            t => panic!("expected name, got {t}"),
        }
    }

    fn parse_atom(&mut self) -> Expr {
        match self.peek().clone() {
            Token::Integer(n) => {
                self.advance();
                Expr::IntLit(n)
            }
            Token::FloatLit(n) => {
                self.advance();
                Expr::FloatLit(n)
            }
            Token::StringLit(s) => {
                self.advance();
                Expr::StringLit(s)
            }
            Token::LParen => {
                self.advance();

                let expr = self.parse_expr();

                self.expect(&Token::RParen);

                expr
            }
            Token::Exit => {
                self.advance();
                self.expect(&Token::LParen);

                let arg = self.parse_expr();

                self.expect(&Token::RParen);

                Expr::Call("Exit".to_string(), vec![arg])
            }
            Token::Ident(name) => {
                self.advance();

                if *self.peek() == Token::LParen {
                    self.advance();

                    let mut args = Vec::new();

                    while *self.peek() != Token::RParen {
                        if !args.is_empty() {
                            self.expect(&Token::Comma);
                        }

                        args.push(self.parse_expr());
                    }

                    self.expect(&Token::RParen);

                    Expr::Call(name, args)
                } else if *self.peek() == Token::ColonColon {
                    self.advance();

                    let func = self.parse_name_or_keyword();

                    self.expect(&Token::LParen);

                    let mut args = Vec::new();

                    while *self.peek() != Token::RParen {
                        if !args.is_empty() {
                            self.expect(&Token::Comma);
                        }

                        args.push(self.parse_expr());
                    }

                    self.expect(&Token::RParen);

                    Expr::ModuleCall(name, func, args)
                } else {
                    Expr::Variable(name)
                }
            }

            t => panic!("unexpected token in expression: {t}"),
        }
    }
}
