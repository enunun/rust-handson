use std::collections::HashMap;

use crate::catalog::{Catalog, Column, SchemaError, TableSchema};
use crate::error::Error;
use crate::exec::eval::{eval, eval_condition};
use crate::plan::binder::{BoundExpr, bind};
use crate::sql::ast::{CreateTable, Expr, Insert, Select, SelectItem, Statement, Values};
use crate::sql::lexer::tokenize;
use crate::sql::parser::parse;
use crate::value::Value;

/// 表の定義と行を持ち，SQLの文を実行するデータベース．
#[derive(Debug, Default)]
pub struct Database {
    catalog: Catalog,
    rows: HashMap<String, Vec<Vec<Value>>>,
}

/// 文を実行した結果．
#[derive(Debug, PartialEq)]
pub enum StatementResult {
    Rows(QueryResult),
    CreateTable,
    Insert { count: usize },
}

/// 問い合わせの結果の表．
#[derive(Debug, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// 名前のない式の結果の列名．
const UNNAMED_COLUMN: &str = "?column?";

impl Database {
    /// 表のない空のデータベースを作る．
    pub fn new() -> Database {
        Database::default()
    }

    /// SQLの文を1つ実行する．
    pub fn execute(&mut self, sql: &str) -> Result<StatementResult, Error> {
        let tokens = tokenize(sql)?;
        match parse(&tokens)? {
            Statement::Values(values) => Ok(StatementResult::Rows(evaluate_values(&values)?)),
            Statement::CreateTable(create) => self.create_table(create),
            Statement::Insert(insert) => self.insert(insert),
            Statement::Select(select) => self.select(&select),
        }
    }

    fn create_table(&mut self, create: CreateTable) -> Result<StatementResult, Error> {
        let columns = create
            .columns
            .into_iter()
            .map(|column| Column {
                name: column.name,
                data_type: column.data_type,
            })
            .collect();
        self.catalog.create_table(TableSchema {
            name: create.name.clone(),
            columns,
        })?;
        self.rows.insert(create.name, Vec::new());
        Ok(StatementResult::CreateTable)
    }

    fn insert(&mut self, insert: Insert) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&insert.table)?;
        let targets = target_columns(schema, insert.columns)?;
        let mut new_rows = Vec::new();
        for exprs in &insert.values.rows {
            if exprs.len() > targets.len() {
                return Err(SchemaError::MoreValuesThanColumns.into());
            }
            if exprs.len() < targets.len() {
                return Err(SchemaError::MoreColumnsThanValues.into());
            }
            let mut row = vec![Value::Null; schema.columns.len()];
            for (expr, &index) in exprs.iter().zip(&targets) {
                row[index] = schema.columns[index].assign(evaluate_constant(expr)?)?;
            }
            new_rows.push(row);
        }
        let count = new_rows.len();
        let table_rows = self
            .rows
            .get_mut(&insert.table)
            .expect("every table in the catalog has its rows");
        table_rows.append(&mut new_rows);
        Ok(StatementResult::Insert { count })
    }

    fn select(&self, select: &Select) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&select.from)?;
        let (columns, exprs) = bind_select_items(&select.items, &schema.columns)?;
        let filter = match &select.filter {
            Some(filter) => Some(bind(filter, &schema.columns)?),
            None => None,
        };
        let mut rows = Vec::new();
        for row in &self.rows[&select.from] {
            if !matches_filter(&filter, row)? {
                continue;
            }
            let values = exprs
                .iter()
                .map(|expr| eval(expr, row))
                .collect::<Result<Vec<_>, _>>()?;
            rows.push(values);
        }
        Ok(StatementResult::Rows(QueryResult { columns, rows }))
    }
}

/// 行が`WHERE`の条件を満たすかを返す．条件がなければ，すべての行が満たす．
fn matches_filter(filter: &Option<BoundExpr>, row: &[Value]) -> Result<bool, Error> {
    match filter {
        Some(filter) => Ok(eval_condition(filter, row, "WHERE")?),
        None => Ok(true),
    }
}

/// 選択項目の名前を解決し，結果の列名と，各列を計算する式を返す．`*`はすべての列に展開する．
fn bind_select_items(
    items: &[SelectItem],
    columns: &[Column],
) -> Result<(Vec<String>, Vec<BoundExpr>), Error> {
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

/// `INSERT`の値を入れる列の番号を，値の順に返す．列を指定しなければ，すべての列を定義の順に返す．
fn target_columns(schema: &TableSchema, names: Option<Vec<String>>) -> Result<Vec<usize>, Error> {
    match names {
        Some(names) => Ok(names
            .iter()
            .map(|name| schema.column_index(name))
            .collect::<Result<Vec<_>, _>>()?),
        None => Ok((0..schema.columns.len()).collect()),
    }
}

/// 列を参照しない式を評価する．
fn evaluate_constant(expr: &Expr) -> Result<Value, Error> {
    Ok(eval(&bind(expr, &[])?, &[])?)
}

/// `VALUES`の各行の式を評価し，結果の表を作る．
fn evaluate_values(values: &Values) -> Result<QueryResult, Error> {
    let rows = values
        .rows
        .iter()
        .map(|exprs| exprs.iter().map(evaluate_constant).collect())
        .collect::<Result<Vec<Vec<Value>>, Error>>()?;
    let columns = (1..=values.rows[0].len())
        .map(|index| format!("COLUMN{index}"))
        .collect();
    Ok(QueryResult { columns, rows })
}
