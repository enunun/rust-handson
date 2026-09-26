use std::collections::HashMap;

use crate::catalog::{Catalog, Column, SchemaError, TableSchema, UniqueConstraint};
use crate::error::Error;
use crate::exec::dml;
use crate::exec::eval::{eval, matches_filter};
use crate::plan::binder::{BoundExpr, bind};
use crate::sql::ast::{
    Assignment, ColumnConstraint, CreateTable, Delete, DropTable, Expr, Insert, Select, SelectItem,
    Statement, Update, Values,
};
use crate::sql::lexer::tokenize;
use crate::sql::parser::parse;
use crate::value::{Row, Value};

/// 表の定義と行を持ち，SQLの文を実行するデータベース．
#[derive(Debug, Default)]
pub struct Database {
    catalog: Catalog,
    rows: HashMap<String, Vec<Row>>,
}

/// 文を実行した結果．
#[derive(Debug, PartialEq)]
pub enum StatementResult {
    Rows(QueryResult),
    CreateTable,
    Insert { count: usize },
    Update { count: usize },
    Delete { count: usize },
    DropTable,
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
            Statement::Update(update) => self.update(&update),
            Statement::Delete(delete) => self.delete(&delete),
            Statement::DropTable(drop) => self.drop_table(&drop),
        }
    }

    fn create_table(&mut self, create: CreateTable) -> Result<StatementResult, Error> {
        let schema = table_schema(create)?;
        let name = schema.name.clone();
        self.catalog.create_table(schema)?;
        self.rows.insert(name, Vec::new());
        Ok(StatementResult::CreateTable)
    }

    fn drop_table(&mut self, drop: &DropTable) -> Result<StatementResult, Error> {
        self.catalog.drop_table(&drop.name)?;
        self.rows.remove(&drop.name);
        Ok(StatementResult::DropTable)
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
        let rows = self
            .rows
            .get_mut(&insert.table)
            .expect("every table in the catalog has its rows");
        let count = dml::insert(schema, rows, new_rows)?;
        Ok(StatementResult::Insert { count })
    }

    fn update(&mut self, update: &Update) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&update.table)?;
        let assignments = bind_assignments(schema, &update.assignments)?;
        let filter = bind_filter(&update.filter, &schema.columns)?;
        let rows = self
            .rows
            .get_mut(&update.table)
            .expect("every table in the catalog has its rows");
        let count = dml::update(schema, rows, &assignments, &filter)?;
        Ok(StatementResult::Update { count })
    }

    fn delete(&mut self, delete: &Delete) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&delete.table)?;
        let filter = bind_filter(&delete.filter, &schema.columns)?;
        let rows = self
            .rows
            .get_mut(&delete.table)
            .expect("every table in the catalog has its rows");
        let count = dml::delete(rows, &filter)?;
        Ok(StatementResult::Delete { count })
    }

    fn select(&self, select: &Select) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&select.from)?;
        let (columns, exprs) = bind_select_items(&select.items, &schema.columns)?;
        let filter = bind_filter(&select.filter, &schema.columns)?;
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

/// `CREATE TABLE`の列の定義から，表の定義を作る．
/// `PRIMARY KEY`の列は`NOT NULL`で，制約の名前は`表_PKEY`である．
/// `UNIQUE`の列の制約の名前は`表_列_KEY`である．
fn table_schema(create: CreateTable) -> Result<TableSchema, SchemaError> {
    let mut columns = Vec::new();
    let mut unique_constraints = Vec::new();
    let mut has_primary_key = false;
    for (index, column) in create.columns.into_iter().enumerate() {
        let primary_key = column.constraints.contains(&ColumnConstraint::PrimaryKey);
        if primary_key {
            if has_primary_key {
                return Err(SchemaError::MultiplePrimaryKeys { table: create.name });
            }
            has_primary_key = true;
            unique_constraints.push(UniqueConstraint {
                name: format!("{}_PKEY", create.name),
                column: index,
            });
        } else if column.constraints.contains(&ColumnConstraint::Unique) {
            unique_constraints.push(UniqueConstraint {
                name: format!("{}_{}_KEY", create.name, column.name),
                column: index,
            });
        }
        let not_null = column.constraints.contains(&ColumnConstraint::NotNull);
        columns.push(Column {
            name: column.name,
            data_type: column.data_type,
            nullable: !primary_key && !not_null,
        });
    }
    Ok(TableSchema {
        name: create.name,
        columns,
        unique_constraints,
    })
}

/// `WHERE`の条件があれば，名前を解決する．
fn bind_filter(filter: &Option<Expr>, columns: &[Column]) -> Result<Option<BoundExpr>, Error> {
    match filter {
        Some(filter) => Ok(Some(bind(filter, columns)?)),
        None => Ok(None),
    }
}

/// `UPDATE`の`列 = 式`の並びを，列の番号と名前を解決した式の組にする．
/// 同じ列に2度代入すればエラーを返す．
fn bind_assignments(
    schema: &TableSchema,
    assignments: &[Assignment],
) -> Result<Vec<(usize, BoundExpr)>, Error> {
    let mut bound = Vec::new();
    for assignment in assignments {
        let index = schema.column_index(&assignment.column)?;
        if bound.iter().any(|(column, _)| *column == index) {
            return Err(SchemaError::DuplicateAssignment {
                column: assignment.column.clone(),
            }
            .into());
        }
        bound.push((index, bind(&assignment.value, &schema.columns)?));
    }
    Ok(bound)
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
