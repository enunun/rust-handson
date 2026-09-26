use std::collections::HashMap;

use crate::catalog::{Catalog, Column, SchemaError, TableSchema};
use crate::error::Error;
use crate::exec::eval::eval;
use crate::sql::ast::{CreateTable, Insert, Select, Statement, Values};
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
        let mut columns = Vec::new();
        for column in create.columns {
            columns.push(Column {
                name: column.name,
                data_type: column.data_type,
            });
        }
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
                row[index] = schema.columns[index].assign(eval(expr)?)?;
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
        let schema = self.catalog.table(&select.table)?;
        let mut columns = Vec::new();
        for column in &schema.columns {
            columns.push(column.name.clone());
        }
        let rows = self.rows[&select.table].clone();
        Ok(StatementResult::Rows(QueryResult { columns, rows }))
    }
}

/// `INSERT`の値を入れる列の番号を，値の順に返す．列を指定しなければ，すべての列を定義の順に返す．
fn target_columns(schema: &TableSchema, names: Option<Vec<String>>) -> Result<Vec<usize>, Error> {
    let mut targets = Vec::new();
    match names {
        Some(names) => {
            for name in names {
                targets.push(schema.column_index(&name)?);
            }
        }
        None => {
            for index in 0..schema.columns.len() {
                targets.push(index);
            }
        }
    }
    Ok(targets)
}

/// `VALUES`の各行の式を評価し，結果の表を作る．
fn evaluate_values(values: &Values) -> Result<QueryResult, Error> {
    let mut rows = Vec::new();
    for exprs in &values.rows {
        let mut row = Vec::new();
        for expr in exprs {
            row.push(eval(expr)?);
        }
        rows.push(row);
    }
    let mut columns = Vec::new();
    for index in 1..=values.rows[0].len() {
        columns.push(format!("COLUMN{index}"));
    }
    Ok(QueryResult { columns, rows })
}
