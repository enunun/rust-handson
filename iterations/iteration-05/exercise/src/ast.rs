/// 式の構文木．
#[derive(Debug, PartialEq)]
pub enum Expr {
    Integer(i64),
    Boolean(bool),
    String(String),
    Null,
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    IsNull {
        operand: Box<Expr>,
        negated: bool,
    },
}

/// 単項演算子．
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

/// 二項演算子．
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Arithmetic(ArithmeticOp),
    Comparison(ComparisonOp),
    Concat,
    And,
    Or,
}

/// 算術演算子．
#[derive(Debug, Clone, PartialEq)]
pub enum ArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// 比較演算子．
#[derive(Debug, Clone, PartialEq)]
pub enum ComparisonOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

/// `VALUES (式, ...)`の構文木．
#[derive(Debug, PartialEq)]
pub struct Values {
    pub exprs: Vec<Expr>,
}
