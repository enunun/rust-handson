use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::catalog::{Catalog, Column, IndexDef, SchemaError, TableSchema, UniqueConstraint};
use crate::error::Error;
use crate::exec::build::{build, collect_rows};
use crate::exec::dml;
use crate::exec::eval::eval;
use crate::index::{AnyIndex, ColumnIndex, check_key};
use crate::plan::binder::{BoundExpr, Scope, bind_filter, bind_in, bind_select};
use crate::plan::explain::explain;
use crate::plan::planner::{PlanNode, plan};
use crate::sql::ast::{
    Assignment, ColumnConstraint, CreateIndex, CreateTable, Delete, DropIndex, DropTable, Expr,
    Insert, Select, Statement, Update, Values,
};
use crate::sql::lexer::tokenize;
use crate::sql::parser::parse;
use crate::storage::buffer::BufferPool;
use crate::storage::disk::{DiskManager, FileDiskManager, MemoryDiskManager};
use crate::storage::heap::{HeapFile, RowId};
use crate::value::{DataType, Row, Value};

/// 表の定義と，表ごとの行を置くページの列と，インデックスごとのB+木のページを持ち，
/// SQLの文を実行するデータベース．
/// `data_dir`があれば，カタログと表とインデックスのページをそのディレクトリのファイルに保存する．
#[derive(Debug, Default)]
pub struct Database {
    catalog: Catalog,
    tables: HashMap<String, HeapFile>,
    indexes: HashMap<String, BufferPool<Box<dyn DiskManager>>>,
    data_dir: Option<PathBuf>,
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
    CreateIndex,
    DropIndex,
}

/// 問い合わせの結果の表．
#[derive(Debug, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// データディレクトリの中の，カタログのファイルの名前．
const CATALOG_FILE: &str = "catalog";

/// 1つのインデックスのバッファプールの枠の数．
const INDEX_BUFFER_FRAMES: usize = 16;

/// `EXPLAIN`の結果の列名．
const QUERY_PLAN_COLUMN: &str = "QUERY PLAN";

impl Database {
    /// 表のない空のデータベースをメモリーに作る．プロセスが終わると消える．
    pub fn new() -> Database {
        Database::default()
    }

    /// データディレクトリのデータベースを開く．ディレクトリがなければ作る．
    /// カタログを読み，カタログにある表のヒープファイルと，インデックスのファイルを開く．
    pub fn open(dir: &Path) -> Result<Database, Error> {
        fs::create_dir_all(dir)?;
        let catalog = Catalog::load(&dir.join(CATALOG_FILE))?;
        let mut tables = HashMap::new();
        let mut indexes = HashMap::new();
        for name in catalog.table_names() {
            let disk = FileDiskManager::open(&file_path(dir, &name, "heap"))?;
            tables.insert(name.clone(), HeapFile::new(Box::new(disk)));
            for index in catalog.indexes_of(&name) {
                let disk = FileDiskManager::open(&file_path(dir, &index.name, "index"))?;
                let disk: Box<dyn DiskManager> = Box::new(disk);
                indexes.insert(
                    index.name.clone(),
                    BufferPool::new(disk, INDEX_BUFFER_FRAMES),
                );
            }
        }
        Ok(Database {
            catalog,
            tables,
            indexes,
            data_dir: Some(dir.to_path_buf()),
        })
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
            Statement::CreateIndex(create) => self.create_index(&create),
            Statement::DropIndex(drop) => self.drop_index(&drop),
            Statement::Explain(select) => self.explain(&select),
        }
    }

    fn create_table(&mut self, create: CreateTable) -> Result<StatementResult, Error> {
        let schema = table_schema(create)?;
        let name = schema.name.clone();
        self.catalog.create_table(schema)?;
        let heap = HeapFile::new(self.new_disk(&name, "heap")?);
        self.tables.insert(name.clone(), heap);
        let schema = self.catalog.table(&name)?;
        for index in self.catalog.indexes_of(&name) {
            let pool = BufferPool::new(self.new_disk(&index.name, "index")?, INDEX_BUFFER_FRAMES);
            AnyIndex::create(&schema.columns[index.column].data_type, &pool)?;
            self.indexes.insert(index.name.clone(), pool);
        }
        self.save_catalog()?;
        Ok(StatementResult::CreateTable)
    }

    fn drop_table(&mut self, drop: &DropTable) -> Result<StatementResult, Error> {
        let index_names: Vec<String> = self
            .catalog
            .indexes_of(&drop.name)
            .iter()
            .map(|index| index.name.clone())
            .collect();
        self.catalog.drop_table(&drop.name)?;
        self.tables.remove(&drop.name);
        for name in &index_names {
            self.indexes.remove(name);
        }
        self.save_catalog()?;
        self.remove_file(&drop.name, "heap")?;
        for name in &index_names {
            self.remove_file(name, "index")?;
        }
        Ok(StatementResult::DropTable)
    }

    /// インデックスを作り，表の今の行をすべて入れる．行を入れられなければ，インデックスを作らない．
    fn create_index(&mut self, create: &CreateIndex) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&create.table)?;
        let column = schema.column_index(&create.column)?;
        let data_type = schema.columns[column].data_type.clone();
        let rows = self.tables[&create.table].rows(schema)?;
        self.catalog.create_index(IndexDef {
            name: create.name.clone(),
            table: create.table.clone(),
            column,
        })?;
        let pool = BufferPool::new(self.new_disk(&create.name, "index")?, INDEX_BUFFER_FRAMES);
        let filled = fill_index(&data_type, &pool, column, rows);
        if let Err(error) = filled {
            self.catalog.drop_index(&create.name)?;
            drop(pool);
            self.remove_file(&create.name, "index")?;
            return Err(error);
        }
        self.indexes.insert(create.name.clone(), pool);
        self.save_catalog()?;
        Ok(StatementResult::CreateIndex)
    }

    fn drop_index(&mut self, drop: &DropIndex) -> Result<StatementResult, Error> {
        self.catalog.drop_index(&drop.name)?;
        self.indexes.remove(&drop.name);
        self.save_catalog()?;
        self.remove_file(&drop.name, "index")?;
        Ok(StatementResult::DropIndex)
    }

    /// データディレクトリがあれば，カタログをファイルに書く．
    fn save_catalog(&self) -> Result<(), Error> {
        if let Some(dir) = &self.data_dir {
            self.catalog.save(&dir.join(CATALOG_FILE))?;
        }
        Ok(())
    }

    /// 表かインデックスのページを置く，空のディスクを作る．データディレクトリがあればファイルを作る．
    fn new_disk(&self, name: &str, extension: &str) -> Result<Box<dyn DiskManager>, Error> {
        Ok(match &self.data_dir {
            Some(dir) => Box::new(FileDiskManager::create(&file_path(dir, name, extension))?),
            None => Box::new(MemoryDiskManager::default()),
        })
    }

    /// データディレクトリがあれば，表かインデックスのファイルを消す．ファイルがなくてもよい．
    fn remove_file(&self, name: &str, extension: &str) -> Result<(), Error> {
        if let Some(dir) = &self.data_dir {
            match fs::remove_file(file_path(dir, name, extension)) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
                _ => {}
            }
        }
        Ok(())
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
        let heap = self
            .tables
            .get_mut(&insert.table)
            .expect("every table in the catalog has its heap file");
        let mut indexes = open_indexes(&self.catalog, &self.indexes, schema)?;
        let count = dml::insert(schema, heap, &mut indexes, new_rows)?;
        Ok(StatementResult::Insert { count })
    }

    fn update(&mut self, update: &Update) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&update.table)?;
        let assignments = bind_assignments(schema, &update.assignments)?;
        let filter = bind_filter(
            &update.filter,
            &Scope::table(&schema.name, &schema.columns),
            "WHERE",
        )?;
        let heap = self
            .tables
            .get_mut(&update.table)
            .expect("every table in the catalog has its heap file");
        let mut indexes = open_indexes(&self.catalog, &self.indexes, schema)?;
        let count = dml::update(schema, heap, &mut indexes, &assignments, &filter)?;
        Ok(StatementResult::Update { count })
    }

    fn delete(&mut self, delete: &Delete) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&delete.table)?;
        let filter = bind_filter(
            &delete.filter,
            &Scope::table(&schema.name, &schema.columns),
            "WHERE",
        )?;
        let heap = self
            .tables
            .get_mut(&delete.table)
            .expect("every table in the catalog has its heap file");
        let mut indexes = open_indexes(&self.catalog, &self.indexes, schema)?;
        let count = dml::delete(schema, heap, &mut indexes, &filter)?;
        Ok(StatementResult::Delete { count })
    }

    fn select(&self, select: &Select) -> Result<StatementResult, Error> {
        let plan = self.plan(select)?;
        let columns = plan.columns().to_vec();
        let mut executor = build(&plan, &self.tables, &self.indexes, &self.catalog)?;
        let rows = collect_rows(executor.as_mut())?;
        Ok(StatementResult::Rows(QueryResult { columns, rows }))
    }

    fn explain(&self, select: &Select) -> Result<StatementResult, Error> {
        let plan = self.plan(select)?;
        let rows = explain(&plan)
            .into_iter()
            .map(|line| vec![Value::Varchar(line)])
            .collect();
        Ok(StatementResult::Rows(QueryResult {
            columns: vec![QUERY_PLAN_COLUMN.to_string()],
            rows,
        }))
    }

    /// `SELECT`の名前を解決し，実行計画を作る．
    fn plan(&self, select: &Select) -> Result<PlanNode, Error> {
        let bound = bind_select(select, &self.catalog)?;
        Ok(plan(&bound, &self.catalog))
    }
}

/// 表のインデックスを，列の型のB+木として開く．
fn open_indexes<'a>(
    catalog: &Catalog,
    pools: &'a HashMap<String, BufferPool<Box<dyn DiskManager>>>,
    schema: &TableSchema,
) -> Result<Vec<ColumnIndex<'a>>, Error> {
    let mut indexes = Vec::new();
    for index in catalog.indexes_of(&schema.name) {
        let data_type = &schema.columns[index.column].data_type;
        indexes.push(ColumnIndex {
            column: index.column,
            index: AnyIndex::open(data_type, &pools[&index.name])?,
        });
    }
    Ok(indexes)
}

/// 空のプールに木を作り，行の`column`番目の列の値と位置を入れる．
fn fill_index(
    data_type: &DataType,
    pool: &BufferPool<Box<dyn DiskManager>>,
    column: usize,
    rows: Vec<(RowId, Row)>,
) -> Result<(), Error> {
    let mut tree = AnyIndex::create(data_type, pool)?;
    for (id, row) in rows {
        check_key(&row[column])?;
        tree.insert(&row[column], id)?;
    }
    Ok(())
}

/// 表かインデックスのファイルのパス．名前をUTF-8のバイトの16進数で書き，ファイル名に使えない文字を避ける．
fn file_path(dir: &Path, name: &str, extension: &str) -> PathBuf {
    let hex: String = name.bytes().map(|byte| format!("{byte:02x}")).collect();
    dir.join(format!("{hex}.{extension}"))
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

/// `UPDATE`の`列 = 式`の並びを，列の番号と名前を解決した式の組にする．
/// 同じ列に2度代入すればエラーを返す．
fn bind_assignments(
    schema: &TableSchema,
    assignments: &[Assignment],
) -> Result<Vec<(usize, BoundExpr)>, Error> {
    let scope = Scope::table(&schema.name, &schema.columns);
    let mut bound = Vec::new();
    for assignment in assignments {
        let index = schema.column_index(&assignment.column)?;
        if bound.iter().any(|(column, _)| *column == index) {
            return Err(SchemaError::DuplicateAssignment {
                column: assignment.column.clone(),
            }
            .into());
        }
        bound.push((index, bind_in(&assignment.value, &scope, "UPDATE")?));
    }
    Ok(bound)
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
    Ok(eval(&bind_in(expr, &Scope::default(), "VALUES")?, &[])?)
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
