use crate::value::DataType;

/// 式の構文木．
#[derive(Debug, PartialEq)]
pub enum Expr {
    Integer(i64),
    Boolean(bool),
    String(String),
    Null,
    Column(String),
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
    Update(Update),
    Delete(Delete),
    DropTable(DropTable),
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
    pub constraints: Vec<ColumnConstraint>,
}

/// 列の制約．
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnConstraint {
    NotNull,
    PrimaryKey,
    Unique,
}

/// `INSERT INTO 表 [(列, ...)] VALUES ...`．列を指定しなければ`columns`は`None`である．
#[derive(Debug, PartialEq)]
pub struct Insert {
    pub table: String,
    pub columns: Option<Vec<String>>,
    pub values: Values,
}

/// `SELECT 選択項目, ... FROM 表 [WHERE 条件]`．
#[derive(Debug, PartialEq)]
pub struct Select {
    pub distinct: bool,
    pub items: Vec<SelectItem>,
    pub from: String,
    pub filter: Option<Expr>,
    pub order_by: Vec<OrderBy>,
    pub limit: Limit,
}

/// `ORDER BY`の並べ替えのキー．`nulls_first`は`NULLS FIRST`なら`Some(true)`，
/// `NULLS LAST`なら`Some(false)`，書かなければ`None`である．
#[derive(Debug, PartialEq)]
pub struct OrderBy {
    pub expr: Expr,
    pub descending: bool,
    pub nulls_first: Option<bool>,
}

/// `OFFSET n ROWS`と`FETCH FIRST n ROWS ONLY`．`offset`行を飛ばし，`fetch`行まで返す．
#[derive(Debug, Default, PartialEq)]
pub struct Limit {
    pub offset: usize,
    pub fetch: Option<usize>,
}

/// `UPDATE 表 SET 列 = 式, ... [WHERE 条件]`．
#[derive(Debug, PartialEq)]
pub struct Update {
    pub table: String,
    pub assignments: Vec<Assignment>,
    pub filter: Option<Expr>,
}

/// `UPDATE`の`列 = 式`．
#[derive(Debug, PartialEq)]
pub struct Assignment {
    pub column: String,
    pub value: Expr,
}

/// `DELETE FROM 表 [WHERE 条件]`．
#[derive(Debug, PartialEq)]
pub struct Delete {
    pub table: String,
    pub filter: Option<Expr>,
}

/// `DROP TABLE 表`．
#[derive(Debug, PartialEq)]
pub struct DropTable {
    pub name: String,
}

/// `SELECT`の選択項目．`*`か，別名を付けられる式である．
#[derive(Debug, PartialEq)]
pub enum SelectItem {
    Wildcard,
    Expr { expr: Expr, alias: Option<String> },
}
