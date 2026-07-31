#[derive(Debug, Clone, PartialEq)]
pub enum Use {
    System,
    Module {
        pub_: bool,
        path: Vec<String>,
    },
    Wildcard {
        pub_: bool,
        path: Vec<String>,
    },
    Item {
        pub_: bool,
        path: Vec<String>,
        name: String,
    },
    Items {
        pub_: bool,
        path: Vec<String>,
        names: Vec<String>,
    },
}

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

#[derive(Debug, Clone)]
pub struct Program {
    pub use_system: bool,
    pub uses: Vec<Use>,
    pub functions: Vec<Function>,
    pub globals: Vec<GlobalVar>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub pub_: bool,
    pub return_type: FllufType,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub mut_: bool,
    pub param_type: FllufType,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct GlobalVar {
    pub pub_: bool,
    pub var_type: FllufType,
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    VarDecl {
        mut_: bool,
        var_type: FllufType,
        name: String,
        value: Expr,
    },
    Assign {
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
    ModuleVar(String, String),
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
