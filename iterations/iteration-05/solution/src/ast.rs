use crate::value::DataType;

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

/// SQLの文．
#[derive(Debug, PartialEq)]
pub enum Statement {
    Values(Values),
    CreateTable(CreateTable),
    Insert(Insert),
    Select(Select),
}

/// `VALUES (式, ...), ...`の構文木．1つの要素が1行である．
#[derive(Debug, PartialEq)]
pub struct Values {
    pub rows: Vec<Vec<Expr>>,
}

/// `CREATE TABLE 表 (列 型, ...)`．
#[derive(Debug, PartialEq)]
pub struct CreateTable {
    pub name: String,
    pub columns: Vec<ColumnDef>,
}

/// `CREATE TABLE`の列の定義．
#[derive(Debug, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: DataType,
}

/// `INSERT INTO 表 [(列, ...)] VALUES ...`．列を指定しなければ`columns`は`None`である．
#[derive(Debug, PartialEq)]
pub struct Insert {
    pub table: String,
    pub columns: Option<Vec<String>>,
    pub values: Values,
}

/// `SELECT * FROM 表`．
#[derive(Debug, PartialEq)]
pub struct Select {
    pub table: String,
}
