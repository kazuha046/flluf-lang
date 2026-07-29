#[derive(Debug, Clone, PartialEq)]
pub enum FllufType {
    Int,
    Float,
    Void,
    String,
}

impl std::fmt::Display for FllufType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FllufType::Int => write!(f, "int"),
            FllufType::Float => write!(f, "float"),
            FllufType::Void => write!(f, "void"),
            FllufType::String => write!(f, "string"),
        }
    }
}

#[derive(Debug)]
pub struct Program {
    pub use_system: bool,
    pub functions: Vec<Function>,
}

#[derive(Debug)]
pub struct Function {
    pub return_type: FllufType,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
}

#[derive(Debug)]
pub struct Param {
    pub param_type: FllufType,
    pub name: String,
}

#[derive(Debug)]
pub struct Block {
    pub statements: Vec<Stmt>,
}

#[derive(Debug)]
pub enum Stmt {
    VarDecl {
        var_type: FllufType,
        name: String,
        value: Expr,
    },
    Return(Expr),
    Expr(Expr),
    If {
        cond: Expr,
        then_block: Block,
        else_ifs: Vec<(Expr, Block)>,
        else_block: Option<Block>,
    },
}

#[derive(Debug, Clone)]
pub enum Expr {
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    Variable(String),
    BinaryOp(Box<Expr>, BinOp, Box<Expr>),
    Call(String, Vec<Expr>),
    ModuleCall(String, String, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}
