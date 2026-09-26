//! 構文木の式の名前を解決する．列の名前を行の中の番号にし，リテラルを値にする．

use crate::catalog::{Column, TableSchema};
use crate::sql::ast::{
    ArithmeticOp, BinaryOp, ComparisonOp, Expr, Limit, Select, SelectItem, UnaryOp,
};
use crate::value::Value;

/// 名前を解決した式．列は行の中の番号で指す．
#[derive(Debug, Clone, PartialEq)]
pub enum BoundExpr {
    Constant(Value),
    Column(usize),
    Unary {
        op: UnaryOp,
        operand: Box<BoundExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<BoundExpr>,
        right: Box<BoundExpr>,
    },
    IsNull {
        operand: Box<BoundExpr>,
        negated: bool,
    },
}

/// 名前を解決した`SELECT`．
#[derive(Debug, PartialEq)]
pub struct BoundSelect {
    pub table: String,
    pub table_columns: Vec<String>,
    pub names: Vec<String>,
    pub items: Vec<BoundExpr>,
    pub filter: Option<BoundExpr>,
    pub distinct: bool,
    pub order_by: Vec<BoundOrderBy>,
    pub limit: Limit,
}

/// 名前を解決した`ORDER BY`のキー．
#[derive(Debug, PartialEq)]
pub struct BoundOrderBy {
    pub source: SortSource,
    pub order: SortOrder,
}

/// 並べ替えのキーの値の求め方．
#[derive(Debug, PartialEq)]
pub enum SortSource {
    /// 結果の列の値を使う．
    Output(usize),
    /// 表の行について式を評価する．
    Input(BoundExpr),
}

/// 並べ替えのキーの向きと，`NULL`を置く位置．
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SortOrder {
    pub descending: bool,
    pub nulls_first: bool,
}

impl SortOrder {
    /// `NULLS FIRST`か`NULLS LAST`を書かなければ，`NULL`を最大の値として扱う．
    /// `ASC`では最後に，`DESC`では最初に置く．
    pub fn new(descending: bool, nulls_first: Option<bool>) -> SortOrder {
        SortOrder {
            descending,
            nulls_first: nulls_first.unwrap_or(descending),
        }
    }
}

/// 名前のない式の結果の列名．
const UNNAMED_COLUMN: &str = "?column?";

/// 名前解決のエラー．
#[derive(Debug, PartialEq)]
pub enum BindError {
    UndefinedColumn { column: String },
    OrderByNotInSelectList,
}

/// `SELECT`の選択項目，`WHERE`の条件，`ORDER BY`のキーの名前を，表の列に解決する．
pub fn bind_select(select: &Select, schema: &TableSchema) -> Result<BoundSelect, BindError> {
    let columns = &schema.columns;
    let (names, items) = bind_select_items(&select.items, columns)?;
    let filter = bind_filter(&select.filter, columns)?;
    let order_by = select
        .order_by
        .iter()
        .map(|key| {
            Ok(BoundOrderBy {
                source: sort_source(&key.expr, &names, &items, columns, select.distinct)?,
                order: SortOrder::new(key.descending, key.nulls_first),
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    Ok(BoundSelect {
        table: schema.name.clone(),
        table_columns: columns.iter().map(|column| column.name.clone()).collect(),
        names,
        items,
        filter,
        distinct: select.distinct,
        order_by,
        limit: select.limit,
    })
}

/// `WHERE`の条件があれば，名前を解決する．
pub fn bind_filter(
    filter: &Option<Expr>,
    columns: &[Column],
) -> Result<Option<BoundExpr>, BindError> {
    match filter {
        Some(filter) => Ok(Some(bind(filter, columns)?)),
        None => Ok(None),
    }
}

/// 選択項目の名前を解決し，結果の列名と，各列を計算する式を返す．`*`はすべての列に展開する．
fn bind_select_items(
    items: &[SelectItem],
    columns: &[Column],
) -> Result<(Vec<String>, Vec<BoundExpr>), BindError> {
    let mut names = Vec::new();
    let mut exprs = Vec::new();
    for item in items {
        match item {
            SelectItem::Wildcard => {
                names.extend(columns.iter().map(|column| column.name.clone()));
                exprs.extend((0..columns.len()).map(BoundExpr::Column));
            }
            SelectItem::Expr { expr, alias } => {
                names.push(column_name(expr, alias));
                exprs.push(bind(expr, columns)?);
            }
        }
    }
    Ok((names, exprs))
}

/// 選択項目の結果の列名．別名があれば別名，列そのものなら列名，それ以外は`?column?`である．
fn column_name(expr: &Expr, alias: &Option<String>) -> String {
    match (alias, expr) {
        (Some(alias), _) => alias.clone(),
        (None, Expr::Column(name)) => name.clone(),
        (None, _) => UNNAMED_COLUMN.to_string(),
    }
}

/// 並べ替えのキーの値の求め方を決める．
/// 名前だけのキーは，まず結果の列名(別名を含む)から探す．見つからなければ，表の列の式として解決する．
/// 選択項目と同じ式なら，その結果の列の値を使う．
/// `DISTINCT`では，結果の列にないキーで並べ替えられない．
fn sort_source(
    expr: &Expr,
    names: &[String],
    items: &[BoundExpr],
    columns: &[Column],
    distinct: bool,
) -> Result<SortSource, BindError> {
    let output = match expr {
        Expr::Column(name) => names.iter().position(|output| output == name),
        _ => None,
    };
    if let Some(index) = output {
        return Ok(SortSource::Output(index));
    }
    let bound = bind(expr, columns)?;
    if let Some(index) = items.iter().position(|item| *item == bound) {
        return Ok(SortSource::Output(index));
    }
    if distinct {
        return Err(BindError::OrderByNotInSelectList);
    }
    Ok(SortSource::Input(bound))
}

/// 式の中の列の名前を，`columns`の中の番号に解決する．
pub fn bind(expr: &Expr, columns: &[Column]) -> Result<BoundExpr, BindError> {
    let bound = match expr {
        Expr::Integer(n) => BoundExpr::Constant(match i32::try_from(*n) {
            Ok(n) => Value::Integer(n),
            Err(_) => Value::BigInt(*n),
        }),
        Expr::Boolean(b) => BoundExpr::Constant(Value::Boolean(*b)),
        Expr::String(s) => BoundExpr::Constant(Value::Varchar(s.clone())),
        Expr::Null => BoundExpr::Constant(Value::Null),
        Expr::Column(name) => {
            let index = columns
                .iter()
                .position(|column| column.name == *name)
                .ok_or_else(|| BindError::UndefinedColumn {
                    column: name.clone(),
                })?;
            BoundExpr::Column(index)
        }
        Expr::Unary { op, operand } => BoundExpr::Unary {
            op: op.clone(),
            operand: Box::new(bind(operand, columns)?),
        },
        Expr::Binary { op, left, right } => BoundExpr::Binary {
            op: op.clone(),
            left: Box::new(bind(left, columns)?),
            right: Box::new(bind(right, columns)?),
        },
        Expr::IsNull { operand, negated } => BoundExpr::IsNull {
            operand: Box::new(bind(operand, columns)?),
            negated: *negated,
        },
    };
    Ok(bound)
}

impl BoundExpr {
    /// `EXPLAIN`に表示する式の文字列．列は`columns`の名前で表し，演算は括弧で囲む．
    pub fn display(&self, columns: &[String]) -> String {
        match self {
            BoundExpr::Constant(value) => display_value(value),
            BoundExpr::Column(index) => columns[*index].clone(),
            BoundExpr::Unary { op, operand } => {
                let operand = operand.display(columns);
                match op {
                    UnaryOp::Neg => format!("(-{operand})"),
                    UnaryOp::Not => format!("(NOT {operand})"),
                }
            }
            BoundExpr::Binary { op, left, right } => format!(
                "({} {} {})",
                left.display(columns),
                operator(op),
                right.display(columns)
            ),
            BoundExpr::IsNull { operand, negated } => {
                let operand = operand.display(columns);
                if *negated {
                    format!("({operand} IS NOT NULL)")
                } else {
                    format!("({operand} IS NULL)")
                }
            }
        }
    }
}

/// 定数をSQLのリテラルの形で表す．
fn display_value(value: &Value) -> String {
    match value {
        Value::Integer(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::Boolean(true) => "TRUE".to_string(),
        Value::Boolean(false) => "FALSE".to_string(),
        Value::Varchar(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Null => "NULL".to_string(),
    }
}

/// 2項演算子の記号．
fn operator(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Arithmetic(ArithmeticOp::Add) => "+",
        BinaryOp::Arithmetic(ArithmeticOp::Sub) => "-",
        BinaryOp::Arithmetic(ArithmeticOp::Mul) => "*",
        BinaryOp::Arithmetic(ArithmeticOp::Div) => "/",
        BinaryOp::Comparison(ComparisonOp::Eq) => "=",
        BinaryOp::Comparison(ComparisonOp::NotEq) => "<>",
        BinaryOp::Comparison(ComparisonOp::Lt) => "<",
        BinaryOp::Comparison(ComparisonOp::LtEq) => "<=",
        BinaryOp::Comparison(ComparisonOp::Gt) => ">",
        BinaryOp::Comparison(ComparisonOp::GtEq) => ">=",
        BinaryOp::Concat => "||",
        BinaryOp::And => "AND",
        BinaryOp::Or => "OR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::ast::Statement;
    use crate::sql::lexer::tokenize;
    use crate::sql::parser::parse;
    use crate::value::DataType;

    fn expr(sql: &str) -> Expr {
        match parse(&tokenize(&format!("VALUES ({sql})")).unwrap()).unwrap() {
            Statement::Values(mut values) => values.rows.remove(0).remove(0),
            other => panic!("not a VALUES statement: {other:?}"),
        }
    }

    fn columns() -> Vec<Column> {
        vec![
            Column {
                name: "ID".to_string(),
                data_type: DataType::Integer,
                nullable: true,
            },
            Column {
                name: "NAME".to_string(),
                data_type: DataType::Varchar(10),
                nullable: true,
            },
        ]
    }

    #[test]
    fn literals_become_constant_values() {
        assert_eq!(
            bind(&expr("1"), &[]),
            Ok(BoundExpr::Constant(Value::Integer(1)))
        );
        assert_eq!(
            bind(&expr("2147483648"), &[]),
            Ok(BoundExpr::Constant(Value::BigInt(2147483648)))
        );
        assert_eq!(
            bind(&expr("'a'"), &[]),
            Ok(BoundExpr::Constant(Value::Varchar("a".to_string())))
        );
        assert_eq!(
            bind(&expr("NULL"), &[]),
            Ok(BoundExpr::Constant(Value::Null))
        );
    }

    #[test]
    fn column_name_becomes_its_index() {
        assert_eq!(bind(&expr("name"), &columns()), Ok(BoundExpr::Column(1)));
    }

    #[test]
    fn columns_inside_an_expression_are_resolved() {
        assert_eq!(
            bind(&expr("id + 1"), &columns()),
            Ok(BoundExpr::Binary {
                op: BinaryOp::Arithmetic(ArithmeticOp::Add),
                left: Box::new(BoundExpr::Column(0)),
                right: Box::new(BoundExpr::Constant(Value::Integer(1)))
            })
        );
    }

    #[test]
    fn unknown_column_is_undefined() {
        assert_eq!(
            bind(&expr("-age"), &columns()),
            Err(BindError::UndefinedColumn {
                column: "AGE".to_string()
            })
        );
    }

    fn select(sql: &str) -> Select {
        match parse(&tokenize(sql).unwrap()).unwrap() {
            Statement::Select(select) => select,
            other => panic!("not a SELECT statement: {other:?}"),
        }
    }

    fn users() -> TableSchema {
        TableSchema {
            name: "USERS".to_string(),
            columns: columns(),
            unique_constraints: vec![],
        }
    }

    fn order_by_sources(sql: &str) -> Result<Vec<SortSource>, BindError> {
        let bound = bind_select(&select(sql), &users())?;
        Ok(bound.order_by.into_iter().map(|key| key.source).collect())
    }

    #[test]
    fn select_items_become_names_and_expressions() {
        let bound = bind_select(&select("SELECT *, id + 1 AS next FROM users"), &users()).unwrap();
        assert_eq!(bound.names, vec!["ID", "NAME", "NEXT"]);
        assert_eq!(bound.items.len(), 3);
        assert_eq!(bound.table_columns, vec!["ID", "NAME"]);
    }

    #[test]
    fn sort_key_prefers_an_output_name_to_a_table_column() {
        assert_eq!(
            order_by_sources("SELECT name AS id FROM users ORDER BY id"),
            Ok(vec![SortSource::Output(0)])
        );
    }

    #[test]
    fn sort_key_equal_to_a_select_item_uses_its_value() {
        assert_eq!(
            order_by_sources("SELECT id + 1 FROM users ORDER BY id + 1"),
            Ok(vec![SortSource::Output(0)])
        );
    }

    #[test]
    fn other_sort_keys_are_evaluated_on_the_table_row() {
        assert_eq!(
            order_by_sources("SELECT name FROM users ORDER BY id"),
            Ok(vec![SortSource::Input(BoundExpr::Column(0))])
        );
        assert_eq!(
            order_by_sources("SELECT DISTINCT name FROM users ORDER BY id"),
            Err(BindError::OrderByNotInSelectList)
        );
    }

    #[test]
    fn expressions_are_displayed_with_column_names_and_parentheses() {
        let names = vec!["ID".to_string(), "NAME".to_string()];
        let display = |sql: &str| bind(&expr(sql), &columns()).unwrap().display(&names);
        assert_eq!(display("id * 10 > 1"), "((ID * 10) > 1)");
        assert_eq!(display("NOT name IS NULL"), "(NOT (NAME IS NULL))");
        assert_eq!(display("-id <> 2147483648"), "((-ID) <> 2147483648)");
        assert_eq!(
            display("name || 'it''s' = TRUE"),
            "((NAME || 'it''s') = TRUE)"
        );
    }
}
