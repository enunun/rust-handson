use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::catalog::{Catalog, Column, IndexDef, SchemaError, TableSchema, UniqueConstraint};
use crate::error::Error;
use crate::exec::build::{BuildContext, build, collect_rows};
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
use crate::storage::buffer::{BufferPool, PageLog};
use crate::storage::disk::{DiskManager, FileDiskManager, MemoryDiskManager};
use crate::storage::heap::{HeapFile, RowId};
use crate::txn::{Snapshot, Transaction, TransactionError, TransactionManager};
use crate::value::{DataType, Row, Value};
use crate::wal::{self, WalRecord, WalWriter};

/// 表の定義と，表ごとの行を置くページの列と，インデックスごとのB+木のページを持ち，
/// SQLの文を実行するデータベース．
/// `data_dir`があれば，カタログと表とインデックスのページをそのディレクトリのファイルに保存する．
#[derive(Debug, Default)]
pub struct Database {
    catalog: Catalog,
    tables: HashMap<String, HeapFile>,
    indexes: HashMap<String, BufferPool<Box<dyn DiskManager>>>,
    data_dir: Option<PathBuf>,
    transactions: TransactionManager,
    session: Session,
    wal: Option<Rc<RefCell<WalWriter<File>>>>,
}

/// セッションの状態．トランザクションの外か，トランザクションの中か，失敗したトランザクションの中である．
#[derive(Debug, Default)]
enum Session {
    #[default]
    Idle,
    InTransaction(Transaction),
    Failed(Transaction),
}

/// セッションのトランザクションの状態．REPLのプロンプトに示す．
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    Idle,
    InTransaction,
    Failed,
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
    StartTransaction,
    Commit,
    Rollback,
    Checkpoint,
}

/// 問い合わせの結果の表．
#[derive(Debug, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// データディレクトリの中の，カタログのファイルの名前．
const CATALOG_FILE: &str = "catalog";

/// データディレクトリの中の，トランザクションの状態のファイルの名前．
const TRANSACTIONS_FILE: &str = "xact";

/// データディレクトリの中の，ログのファイルの名前．
const WAL_FILE: &str = "wal";

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
    /// ログを最後のチェックポイントからやり直してから，カタログを読み，カタログにある表のヒープファイルと，
    /// インデックスのファイルを開く．最後にチェックポイントを取る．
    pub fn open(dir: &Path) -> Result<Database, Error> {
        fs::create_dir_all(dir)?;
        let mut transactions = TransactionManager::load(&dir.join(TRANSACTIONS_FILE))?;
        let wal_path = dir.join(WAL_FILE);
        let end = wal::recover(dir, &wal_path, &mut transactions)?;
        let wal = WalWriter::open(&wal_path, end)?;
        let mut database = Database {
            catalog: Catalog::load(&dir.join(CATALOG_FILE))?,
            data_dir: Some(dir.to_path_buf()),
            transactions,
            wal: Some(Rc::new(RefCell::new(wal))),
            ..Database::default()
        };
        for name in database.catalog.table_names() {
            let heap = database.heap_file(&name, false)?;
            database.tables.insert(name.clone(), heap);
            for index in database.catalog.indexes_of(&name) {
                let pool = database.index_pool(&index.name, false)?;
                database.indexes.insert(index.name.clone(), pool);
            }
        }
        database.checkpoint()?;
        Ok(database)
    }

    /// セッションのトランザクションの状態．
    pub fn transaction_status(&self) -> TransactionStatus {
        match self.session {
            Session::Idle => TransactionStatus::Idle,
            Session::InTransaction(_) => TransactionStatus::InTransaction,
            Session::Failed(_) => TransactionStatus::Failed,
        }
    }

    /// SQLの文を1つ実行する．トランザクションの外の文は，それだけを1つのトランザクションとして実行し，
    /// 成功すればコミットし，失敗すれば中止する．
    pub fn execute(&mut self, sql: &str) -> Result<StatementResult, Error> {
        let statement = tokenize(sql)
            .map_err(Error::from)
            .and_then(|tokens| Ok(parse(&tokens)?));
        match std::mem::take(&mut self.session) {
            Session::Idle => match statement? {
                Statement::StartTransaction => {
                    self.session = Session::InTransaction(self.begin()?);
                    Ok(StatementResult::StartTransaction)
                }
                Statement::Commit => Ok(StatementResult::Commit),
                Statement::Rollback => Ok(StatementResult::Rollback),
                statement => {
                    let txn = self.begin()?;
                    let result = self.run(statement, &txn);
                    match result {
                        Ok(_) => self.commit(txn)?,
                        Err(_) => self.rollback(txn),
                    }
                    result
                }
            },
            Session::InTransaction(txn) => match statement {
                Ok(Statement::StartTransaction) => {
                    self.session = Session::InTransaction(txn);
                    Err(TransactionError::AlreadyInProgress.into())
                }
                Ok(Statement::Commit) => {
                    self.commit(txn)?;
                    Ok(StatementResult::Commit)
                }
                Ok(Statement::Rollback) => {
                    self.rollback(txn);
                    Ok(StatementResult::Rollback)
                }
                Ok(statement) => {
                    let result = in_block(&statement).and_then(|()| self.run(statement, &txn));
                    self.session = match result {
                        Ok(_) => Session::InTransaction(txn),
                        Err(_) => Session::Failed(txn),
                    };
                    result
                }
                Err(error) => {
                    self.session = Session::Failed(txn);
                    Err(error)
                }
            },
            Session::Failed(txn) => match statement {
                Ok(Statement::Commit | Statement::Rollback) => {
                    self.rollback(txn);
                    Ok(StatementResult::Rollback)
                }
                _ => {
                    self.session = Session::Failed(txn);
                    Err(TransactionError::InFailedTransaction.into())
                }
            },
        }
    }

    /// 文を`txn`のトランザクションで実行する．文ごとに新しいスナップショットを使う．
    fn run(&mut self, statement: Statement, txn: &Transaction) -> Result<StatementResult, Error> {
        let snapshot = self.transactions.snapshot(txn.xid());
        match statement {
            Statement::Values(values) => Ok(StatementResult::Rows(evaluate_values(&values)?)),
            Statement::CreateTable(create) => self.create_table(create),
            Statement::Insert(insert) => self.insert(insert, &snapshot),
            Statement::Select(select) => self.select(&select, &snapshot),
            Statement::Update(update) => self.update(&update, &snapshot),
            Statement::Delete(delete) => self.delete(&delete, &snapshot),
            Statement::DropTable(drop) => self.drop_table(&drop),
            Statement::CreateIndex(create) => self.create_index(&create),
            Statement::DropIndex(drop) => self.drop_index(&drop),
            Statement::Explain(select) => self.explain(&select),
            Statement::Checkpoint => {
                self.checkpoint()?;
                Ok(StatementResult::Checkpoint)
            }
            Statement::StartTransaction | Statement::Commit | Statement::Rollback => {
                unreachable!("transaction statements are handled by execute")
            }
        }
    }

    /// トランザクションを始め，開始をログに書く．プロセスが途中で止まっても，
    /// ログをやり直すときに，このトランザクションの番号を中止したものとして読める．
    fn begin(&mut self) -> Result<Transaction, Error> {
        let txn = self.transactions.begin();
        if let Some(wal) = &self.wal
            && let Err(error) = wal
                .borrow_mut()
                .append(&WalRecord::Begin { xid: txn.xid() })
        {
            txn.rollback(&mut self.transactions);
            return Err(error.into());
        }
        Ok(txn)
    }

    /// コミットをログに書き，ログがディスクに届いてから，トランザクションをコミット済みにする．
    /// ログに書けなければ，トランザクションを中止してエラーを返す．
    fn commit(&mut self, txn: Transaction) -> Result<(), Error> {
        if let Some(wal) = &self.wal {
            let mut wal = wal.borrow_mut();
            let written = wal
                .append(&WalRecord::Commit { xid: txn.xid() })
                .and_then(|lsn| wal.flush_to(lsn));
            if let Err(error) = written {
                drop(wal);
                txn.rollback(&mut self.transactions);
                return Err(error.into());
            }
        }
        txn.commit(&mut self.transactions);
        Ok(())
    }

    /// トランザクションを中止する．中止の記録は，ログのやり直しで中止を知るための目印で，
    /// 書けなくても，コミットの記録がなければ中止したものとみなされる．
    fn rollback(&mut self, txn: Transaction) {
        if let Some(wal) = &self.wal {
            let _ = wal
                .borrow_mut()
                .append(&WalRecord::Abort { xid: txn.xid() });
        }
        txn.rollback(&mut self.transactions);
    }

    /// 変更されたページをすべてファイルに書き戻してディスクに届けてから，トランザクションの状態を
    /// ファイルに書き，チェックポイントをログに記録する．データディレクトリがなければ何もしない．
    fn checkpoint(&mut self) -> Result<(), Error> {
        let (Some(dir), Some(wal)) = (&self.data_dir, &self.wal) else {
            return Ok(());
        };
        for heap in self.tables.values() {
            heap.flush()?;
        }
        for pool in self.indexes.values() {
            pool.flush_all()?;
        }
        self.transactions.save(&dir.join(TRANSACTIONS_FILE))?;
        let mut wal = wal.borrow_mut();
        let lsn = wal.append(&WalRecord::Checkpoint)?;
        wal.flush_to(lsn)?;
        Ok(())
    }

    fn create_table(&mut self, create: CreateTable) -> Result<StatementResult, Error> {
        let schema = table_schema(create)?;
        let name = schema.name.clone();
        self.catalog.create_table(schema)?;
        let heap = self.heap_file(&name, true)?;
        self.tables.insert(name.clone(), heap);
        let schema = self.catalog.table(&name)?;
        for index in self.catalog.indexes_of(&name) {
            let pool = self.index_pool(&index.name, true)?;
            AnyIndex::create(&schema.columns[index.column].data_type, &pool)?;
            self.indexes.insert(index.name.clone(), pool);
        }
        self.save_catalog()?;
        self.checkpoint()?;
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
        self.checkpoint()?;
        Ok(StatementResult::DropTable)
    }

    /// インデックスを作り，表の今の行をすべて入れる．行を入れられなければ，インデックスを作らない．
    fn create_index(&mut self, create: &CreateIndex) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&create.table)?;
        let column = schema.column_index(&create.column)?;
        let data_type = schema.columns[column].data_type.clone();
        let rows = self.tables[&create.table].rows(schema, |_| true)?;
        self.catalog.create_index(IndexDef {
            name: create.name.clone(),
            table: create.table.clone(),
            column,
        })?;
        let pool = self.index_pool(&create.name, true)?;
        let filled = fill_index(&data_type, &pool, column, rows);
        if let Err(error) = filled {
            self.catalog.drop_index(&create.name)?;
            drop(pool);
            self.remove_file(&create.name, "index")?;
            return Err(error);
        }
        self.indexes.insert(create.name.clone(), pool);
        self.save_catalog()?;
        self.checkpoint()?;
        Ok(StatementResult::CreateIndex)
    }

    fn drop_index(&mut self, drop: &DropIndex) -> Result<StatementResult, Error> {
        self.catalog.drop_index(&drop.name)?;
        self.indexes.remove(&drop.name);
        self.save_catalog()?;
        self.remove_file(&drop.name, "index")?;
        self.checkpoint()?;
        Ok(StatementResult::DropIndex)
    }

    /// データディレクトリがあれば，カタログをファイルに書く．
    fn save_catalog(&self) -> Result<(), Error> {
        if let Some(dir) = &self.data_dir {
            self.catalog.save(&dir.join(CATALOG_FILE))?;
        }
        Ok(())
    }

    /// 表のページの列．データディレクトリがあれば，ファイルに置き，書き換えたページをログに記録する．
    /// `create`が真なら空のファイルを作り，偽ならあるファイルを開く．
    fn heap_file(&self, table: &str, create: bool) -> Result<HeapFile, Error> {
        Ok(match self.disk(table, "heap", create)? {
            (disk, Some(log)) => HeapFile::with_log(disk, log),
            (disk, None) => HeapFile::new(disk),
        })
    }

    /// インデックスのページのプール．`heap_file`と同じく，データディレクトリではログに記録する．
    fn index_pool(
        &self,
        index: &str,
        create: bool,
    ) -> Result<BufferPool<Box<dyn DiskManager>>, Error> {
        Ok(match self.disk(index, "index", create)? {
            (disk, Some(log)) => BufferPool::with_log(disk, INDEX_BUFFER_FRAMES, log),
            (disk, None) => BufferPool::new(disk, INDEX_BUFFER_FRAMES),
        })
    }

    /// 表かインデックスのページを置くディスクと，書き換えを記録するログ．
    /// データディレクトリがなければ，メモリーに置き，ログに記録しない．
    fn disk(
        &self,
        name: &str,
        extension: &str,
        create: bool,
    ) -> Result<(Box<dyn DiskManager>, Option<PageLog>), Error> {
        let (Some(dir), Some(wal)) = (&self.data_dir, &self.wal) else {
            return Ok((Box::new(MemoryDiskManager::default()), None));
        };
        let file = file_name(name, extension);
        let path = dir.join(&file);
        let disk = if create {
            FileDiskManager::create(&path)?
        } else {
            FileDiskManager::open(&path)?
        };
        let log = PageLog {
            wal: Rc::clone(wal),
            file,
        };
        Ok((Box::new(disk), Some(log)))
    }

    /// データディレクトリがあれば，表かインデックスのファイルを消す．ファイルがなくてもよい．
    fn remove_file(&self, name: &str, extension: &str) -> Result<(), Error> {
        if let Some(dir) = &self.data_dir {
            match fs::remove_file(dir.join(file_name(name, extension))) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
                _ => {}
            }
        }
        Ok(())
    }

    fn insert(&mut self, insert: Insert, snapshot: &Snapshot) -> Result<StatementResult, Error> {
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
        let count = dml::insert(
            schema,
            heap,
            &mut indexes,
            snapshot,
            &self.transactions,
            new_rows,
        )?;
        Ok(StatementResult::Insert { count })
    }

    fn update(&mut self, update: &Update, snapshot: &Snapshot) -> Result<StatementResult, Error> {
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
        let count = dml::update(
            schema,
            heap,
            &mut indexes,
            snapshot,
            &self.transactions,
            &assignments,
            &filter,
        )?;
        Ok(StatementResult::Update { count })
    }

    fn delete(&mut self, delete: &Delete, snapshot: &Snapshot) -> Result<StatementResult, Error> {
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
        let count = dml::delete(schema, heap, snapshot, &self.transactions, &filter)?;
        Ok(StatementResult::Delete { count })
    }

    fn select(&self, select: &Select, snapshot: &Snapshot) -> Result<StatementResult, Error> {
        let plan = self.plan(select)?;
        let columns = plan.columns().to_vec();
        let context = BuildContext {
            tables: &self.tables,
            indexes: &self.indexes,
            catalog: &self.catalog,
            snapshot,
            manager: &self.transactions,
        };
        let mut executor = build(&plan, &context)?;
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

/// 表かインデックスのファイルの名前．名前をUTF-8のバイトの16進数で書き，ファイル名に使えない文字を避ける．
fn file_name(name: &str, extension: &str) -> String {
    let hex: String = name.bytes().map(|byte| format!("{byte:02x}")).collect();
    format!("{hex}.{extension}")
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

/// トランザクションの中で実行できない文ならエラーを返す．表とインデックスを作る文と消す文は，
/// トランザクションの外で実行する．
fn in_block(statement: &Statement) -> Result<(), Error> {
    let name = match statement {
        Statement::CreateTable(_) => "CREATE TABLE",
        Statement::DropTable(_) => "DROP TABLE",
        Statement::CreateIndex(_) => "CREATE INDEX",
        Statement::DropIndex(_) => "DROP INDEX",
        _ => return Ok(()),
    };
    Err(TransactionError::NotInTransactionBlock { statement: name }.into())
}
