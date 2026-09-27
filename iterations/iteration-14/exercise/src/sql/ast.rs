use crate::value::DataType;

/// 式の構文木．
#[derive(Debug, PartialEq)]
pub enum Expr {
    Integer(i64),
    Boolean(bool),
    String(String),
    Null,
    Column(String),
    QualifiedColumn {
        table: String,
        column: String,
    },
    /// 集約関数の呼び出し．`arg`は`COUNT(*)`なら`None`である．
    Aggregate {
        func: AggregateFunc,
        arg: Option<Box<Expr>>,
        distinct: bool,
    },
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
    Explain(Select),
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
    pub from: TableRef,
    pub filter: Option<Expr>,
    pub group_by: Vec<Expr>,
    pub having: Option<Expr>,
    pub order_by: Vec<OrderBy>,
    pub limit: Limit,
}

/// 集約関数．
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AggregateFunc {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl AggregateFunc {
    /// 関数の名前．結果の列名と`EXPLAIN`の表示に使う．
    pub fn name(self) -> &'static str {
        match self {
            AggregateFunc::Count => "COUNT",
            AggregateFunc::Sum => "SUM",
            AggregateFunc::Avg => "AVG",
            AggregateFunc::Min => "MIN",
            AggregateFunc::Max => "MAX",
        }
    }
}

impl Expr {
    /// 式の中に集約関数の呼び出しがあるかを返す．
    pub fn contains_aggregate(&self) -> bool {
        match self {
            Expr::Aggregate { .. } => true,
            Expr::Unary { operand, .. } | Expr::IsNull { operand, .. } => {
                operand.contains_aggregate()
            }
            Expr::Binary { left, right, .. } => {
                left.contains_aggregate() || right.contains_aggregate()
            }
            Expr::Integer(_)
            | Expr::Boolean(_)
            | Expr::String(_)
            | Expr::Null
            | Expr::Column(_)
            | Expr::QualifiedColumn { .. } => false,
        }
    }
}

/// `FROM`に書く表．表そのものか，2つの表の結合である．
#[derive(Debug, PartialEq)]
pub enum TableRef {
    Table { name: String, alias: Option<String> },
    Join(Box<Join>),
}

/// 2つの表の結合．`condition`は`ON`の条件で，`CROSS JOIN`と`,`では`None`である．
#[derive(Debug, PartialEq)]
pub struct Join {
    pub left: TableRef,
    pub right: TableRef,
    pub kind: JoinKind,
    pub condition: Option<Expr>,
}

/// 結合の種類．
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinKind {
    Cross,
    Inner,
    Left,
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
#[derive(Debug, Default, Clone, Copy, PartialEq)]
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
