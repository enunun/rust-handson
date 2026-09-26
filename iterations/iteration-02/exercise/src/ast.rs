/// 式の構文木．
#[derive(Debug, PartialEq)]
pub enum Expr {
    Integer(i64),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

/// 単項演算子．
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
}

/// 二項演算子．
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// `VALUES (式, ...)`の構文木．
#[derive(Debug, PartialEq)]
pub struct Values {
    pub exprs: Vec<Expr>,
}
