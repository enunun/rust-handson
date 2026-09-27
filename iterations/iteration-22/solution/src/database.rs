use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

use crate::catalog::{Catalog, Column, IndexDef, SchemaError, TableSchema, UniqueConstraint};
use crate::error::Error;
use crate::exec::build::{BuildContext, build, collect_rows};
use crate::exec::dml::{self, Outcome};
use crate::exec::eval::eval;
use crate::index::{AnyIndex, ColumnIndex, check_key};
use crate::plan::binder::{BoundExpr, Scope, bind_filter, bind_in, bind_select};
use crate::plan::explain::explain;
use crate::plan::planner::{PlanNode, plan};
use crate::sql::ast::{
    Assignment, ColumnConstraint, CreateIndex, CreateTable, Delete, DropIndex, DropTable, Expr,
    Insert, IsolationLevel, Select, Statement, Update, Values,
};
use crate::sql::lexer::tokenize;
use crate::sql::parser::parse;
use crate::storage::buffer::{BufferPool, PageLog};
use crate::storage::disk::{DiskManager, FileDiskManager, MemoryDiskManager};
use crate::storage::heap::{HeapFile, RowId};
use crate::txn::{
    EndSignal, Isolation, Snapshot, Transaction, TransactionError, TransactionManager, TxnId,
    TxnStatus,
};
use crate::value::{DataType, Row, Value};
use crate::wal::{self, WalRecord, WalWriter};

/// 複数のセッションが`Arc`で共有するデータベース．
/// 表の定義とページを`storage`に，トランザクションの状態を`transactions`に持ち，それぞれをラッチで守る．
/// `data_dir`があれば，カタログと表とインデックスのページをそのディレクトリのファイルに保存する．
/// 行を書き換えるセッションは，その行を書き換えている進行中のトランザクションの終わりを`ends`で待つ．
/// 待つ時間は`lock_timeout`までである．
#[derive(Debug)]
pub struct Database {
    storage: RwLock<Storage>,
    transactions: RwLock<TransactionManager>,
    data_dir: Option<PathBuf>,
    wal: Option<Arc<Mutex<WalWriter<File>>>>,
    ends: EndSignal,
    lock_timeout: Duration,
}

impl Default for Database {
    fn default() -> Database {
        Database {
            storage: RwLock::default(),
            transactions: RwLock::default(),
            data_dir: None,
            wal: None,
            ends: EndSignal::default(),
            lock_timeout: DEFAULT_LOCK_TIMEOUT,
        }
    }
}

/// 表の定義と，表ごとの行を置くページの列と，インデックスごとのB+木のページ．
/// 文を実行する間，`Database`の`RwLock`で守る．
#[derive(Debug, Default)]
struct Storage {
    catalog: Catalog,
    tables: HashMap<String, HeapFile>,
    indexes: HashMap<String, BufferPool<Box<dyn DiskManager>>>,
}

/// 1つの接続のセッション．共有するデータベースと，このセッションのトランザクションの状態を持つ．
/// トランザクションの中で捨てると，トランザクションを中止する．
#[derive(Debug)]
pub struct Session {
    database: Arc<Database>,
    state: SessionState,
}

/// セッションの状態．トランザクションの外か，トランザクションの中か，失敗したトランザクションの中である．
#[derive(Debug, Default)]
enum SessionState {
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

/// ほかのトランザクションの終わりを待つ時間の上限の既定値．
const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(60);

/// `EXPLAIN`の結果の列名．
const QUERY_PLAN_COLUMN: &str = "QUERY PLAN";

/// ラッチを取れないときのメッセージ．ほかのスレッドがラッチを持ったままパニックしていた．
const POISONED: &str = "a thread panicked while holding the latch";

impl Session {
    /// `database`を使うセッションを，トランザクションの外で始める．
    pub fn new(database: Arc<Database>) -> Session {
        Session {
            database,
            state: SessionState::Idle,
        }
    }

    /// セッションのトランザクションの状態．
    pub fn transaction_status(&self) -> TransactionStatus {
        match self.state {
            SessionState::Idle => TransactionStatus::Idle,
            SessionState::InTransaction(_) => TransactionStatus::InTransaction,
            SessionState::Failed(_) => TransactionStatus::Failed,
        }
    }

    /// SQLの文を1つ実行する．トランザクションの外の文は，それだけを1つのトランザクションとして実行し，
    /// 成功すればコミットし，失敗すれば中止する．
    pub fn execute(&mut self, sql: &str) -> Result<StatementResult, Error> {
        let statement = tokenize(sql)
            .map_err(Error::from)
            .and_then(|tokens| Ok(parse(&tokens)?));
        let database = &self.database;
        match std::mem::take(&mut self.state) {
            SessionState::Idle => match statement? {
                Statement::StartTransaction(level) => {
                    let isolation = match level {
                        IsolationLevel::ReadCommitted => Isolation::ReadCommitted,
                        IsolationLevel::RepeatableRead => Isolation::RepeatableRead,
                        IsolationLevel::Serializable => {
                            return Err(TransactionError::SerializableNotSupported.into());
                        }
                    };
                    self.state = SessionState::InTransaction(database.begin(isolation)?);
                    Ok(StatementResult::StartTransaction)
                }
                Statement::Commit => Ok(StatementResult::Commit),
                Statement::Rollback => Ok(StatementResult::Rollback),
                statement => {
                    let mut txn = database.begin(Isolation::ReadCommitted)?;
                    let result = database.run(statement, &mut txn);
                    match result {
                        Ok(_) => database.commit(txn)?,
                        Err(_) => database.rollback(txn),
                    }
                    result
                }
            },
            SessionState::InTransaction(mut txn) => match statement {
                Ok(Statement::StartTransaction(_)) => {
                    self.state = SessionState::InTransaction(txn);
                    Err(TransactionError::AlreadyInProgress.into())
                }
                Ok(Statement::Commit) => {
                    database.commit(txn)?;
                    Ok(StatementResult::Commit)
                }
                Ok(Statement::Rollback) => {
                    database.rollback(txn);
                    Ok(StatementResult::Rollback)
                }
                Ok(statement) => {
                    let result =
                        in_block(&statement).and_then(|()| database.run(statement, &mut txn));
                    self.state = match result {
                        Ok(_) => SessionState::InTransaction(txn),
                        Err(_) => SessionState::Failed(txn),
                    };
                    result
                }
                Err(error) => {
                    self.state = SessionState::Failed(txn);
                    Err(error)
                }
            },
            SessionState::Failed(txn) => match statement {
                Ok(Statement::Commit | Statement::Rollback) => {
                    database.rollback(txn);
                    Ok(StatementResult::Rollback)
                }
                _ => {
                    self.state = SessionState::Failed(txn);
                    Err(TransactionError::InFailedTransaction.into())
                }
            },
        }
    }
}

/// 接続が切れるなどして，トランザクションの中でセッションを捨てたら，トランザクションを中止する．
impl Drop for Session {
    fn drop(&mut self) {
        match std::mem::take(&mut self.state) {
            SessionState::Idle => {}
            SessionState::InTransaction(txn) | SessionState::Failed(txn) => {
                self.database.rollback(txn);
            }
        }
    }
}

impl Database {
    /// 表のない空のデータベースをメモリーに作る．プロセスが終わると消える．
    pub fn new() -> Database {
        Database::default()
    }

    /// ほかのトランザクションの終わりを待つ時間の上限を`timeout`にする．上限を過ぎると，文はエラーになる．
    pub fn with_lock_timeout(self, timeout: Duration) -> Database {
        Database {
            lock_timeout: timeout,
            ..self
        }
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
        let database = Database {
            transactions: RwLock::new(transactions),
            data_dir: Some(dir.to_path_buf()),
            wal: Some(Arc::new(Mutex::new(wal))),
            ..Database::default()
        };
        let mut storage = Storage {
            catalog: Catalog::load(&dir.join(CATALOG_FILE))?,
            ..Storage::default()
        };
        for name in storage.catalog.table_names() {
            let heap = database.heap_file(&name, false)?;
            storage.tables.insert(name.clone(), heap);
            for index in storage.catalog.indexes_of(&name) {
                let pool = database.index_pool(&index.name, false)?;
                storage.indexes.insert(index.name.clone(), pool);
            }
        }
        database.checkpoint(&storage, &database.read_transactions())?;
        *database.write_storage() = storage;
        Ok(database)
    }

    fn read_storage(&self) -> RwLockReadGuard<'_, Storage> {
        self.storage.read().expect(POISONED)
    }

    fn write_storage(&self) -> RwLockWriteGuard<'_, Storage> {
        self.storage.write().expect(POISONED)
    }

    fn read_transactions(&self) -> RwLockReadGuard<'_, TransactionManager> {
        self.transactions.read().expect(POISONED)
    }

    fn write_transactions(&self) -> RwLockWriteGuard<'_, TransactionManager> {
        self.transactions.write().expect(POISONED)
    }

    /// ログのファイル．データディレクトリがなければ`None`を返す．
    fn lock_wal(&self) -> Option<MutexGuard<'_, WalWriter<File>>> {
        self.wal.as_ref().map(|wal| wal.lock().expect(POISONED))
    }

    /// 文を`txn`のトランザクションで実行する．スナップショットは，分離レベルに合わせて`txn`が選ぶ．
    /// 問い合わせは`storage`の読み取りのラッチを取るので，ほかのセッションの問い合わせと同時に動く．
    /// 表を変える文は書き込みのラッチを取るので，1つずつ動く．ラッチは文を終えると外す．
    fn run(&self, statement: Statement, txn: &mut Transaction) -> Result<StatementResult, Error> {
        match statement {
            Statement::Values(values) => Ok(StatementResult::Rows(evaluate_values(&values)?)),
            Statement::Select(select) => {
                let storage = self.read_storage();
                let manager = self.read_transactions();
                let snapshot = txn.snapshot(&manager);
                select_rows(&storage, &select, &snapshot, &manager)
            }
            Statement::Explain(select) => explain_plan(&self.read_storage(), &select),
            Statement::Update(update) => {
                let count = self.change_rows(txn, |storage, snapshot, manager| {
                    storage.update(&update, snapshot, manager)
                })?;
                Ok(StatementResult::Update { count })
            }
            Statement::Delete(delete) => {
                let count = self.change_rows(txn, |storage, snapshot, manager| {
                    storage.delete(&delete, snapshot, manager)
                })?;
                Ok(StatementResult::Delete { count })
            }
            statement => {
                let mut storage = self.write_storage();
                let manager = self.read_transactions();
                let snapshot = txn.snapshot(&manager);
                self.change(&mut storage, statement, &snapshot, &manager)
            }
        }
    }

    /// 行を書き換える文を`change`で実行し，書き換えた行の数を返す．
    /// 書き換える行を進行中のほかのトランザクションが書き換えていれば，ラッチをすべて外して
    /// そのトランザクションの終わりを待ち，文をやり直す．
    fn change_rows(
        &self,
        txn: &mut Transaction,
        change: impl Fn(&mut Storage, &Snapshot, &TransactionManager) -> Result<Outcome, Error>,
    ) -> Result<usize, Error> {
        loop {
            let outcome = {
                let mut storage = self.write_storage();
                let manager = self.read_transactions();
                let snapshot = txn.snapshot(&manager);
                change(&mut storage, &snapshot, &manager)?
            };
            match outcome {
                Outcome::Done(count) => return Ok(count),
                Outcome::WaitFor(xid) => self.wait_for(xid)?,
            }
        }
    }

    /// `xid`のトランザクションが終わるまで待つ．`lock_timeout`を過ぎても終わらなければ，エラーを返す．
    fn wait_for(&self, xid: TxnId) -> Result<(), Error> {
        let ended = self.ends.wait_until(self.lock_timeout, || {
            self.read_transactions().status(xid) != TxnStatus::InProgress
        });
        if ended {
            Ok(())
        } else {
            Err(TransactionError::LockTimeout.into())
        }
    }

    /// 表やインデックスを変える文を，書き込みのラッチを取った`storage`で実行する．
    fn change(
        &self,
        storage: &mut Storage,
        statement: Statement,
        snapshot: &Snapshot,
        manager: &TransactionManager,
    ) -> Result<StatementResult, Error> {
        match statement {
            Statement::CreateTable(create) => self.create_table(storage, create, manager),
            Statement::Insert(insert) => storage.insert(insert, snapshot, manager),
            Statement::DropTable(drop) => self.drop_table(storage, &drop, manager),
            Statement::CreateIndex(create) => self.create_index(storage, &create, manager),
            Statement::DropIndex(drop) => self.drop_index(storage, &drop, manager),
            Statement::Checkpoint => {
                self.checkpoint(storage, manager)?;
                Ok(StatementResult::Checkpoint)
            }
            Statement::Values(_)
            | Statement::Select(_)
            | Statement::Explain(_)
            | Statement::Update(_)
            | Statement::Delete(_) => {
                unreachable!("queries and row changes are handled by run")
            }
            Statement::StartTransaction(_) | Statement::Commit | Statement::Rollback => {
                unreachable!("transaction statements are handled by execute")
            }
        }
    }

    /// トランザクションを始め，開始をログに書く．プロセスが途中で止まっても，
    /// ログをやり直すときに，このトランザクションの番号を中止したものとして読める．
    fn begin(&self, isolation: Isolation) -> Result<Transaction, Error> {
        let txn = self.write_transactions().begin_with(isolation);
        if let Some(mut wal) = self.lock_wal()
            && let Err(error) = wal.append(&WalRecord::Begin { xid: txn.xid() })
        {
            drop(wal);
            self.abort(txn);
            return Err(error.into());
        }
        Ok(txn)
    }

    /// コミットをログに書き，ログがディスクに届いてから，トランザクションをコミット済みにする．
    /// ログに書けなければ，トランザクションを中止してエラーを返す．
    /// どちらの場合も，終わりを待っているセッションを起こす．
    fn commit(&self, txn: Transaction) -> Result<(), Error> {
        if let Some(mut wal) = self.lock_wal() {
            let written = wal
                .append(&WalRecord::Commit { xid: txn.xid() })
                .and_then(|lsn| wal.flush_to(lsn));
            if let Err(error) = written {
                drop(wal);
                self.abort(txn);
                return Err(error.into());
            }
        }
        txn.commit(&mut self.write_transactions());
        self.ends.notify();
        Ok(())
    }

    /// トランザクションを中止する．中止の記録は，ログのやり直しで中止を知るための目印で，
    /// 書けなくても，コミットの記録がなければ中止したものとみなされる．
    fn rollback(&self, txn: Transaction) {
        if let Some(mut wal) = self.lock_wal() {
            let _ = wal.append(&WalRecord::Abort { xid: txn.xid() });
        }
        self.abort(txn);
    }

    /// トランザクションを中止済みにして，終わりを待っているセッションを起こす．
    fn abort(&self, txn: Transaction) {
        txn.rollback(&mut self.write_transactions());
        self.ends.notify();
    }

    /// 変更されたページをすべてファイルに書き戻してディスクに届けてから，トランザクションの状態を
    /// ファイルに書き，チェックポイントをログに記録する．データディレクトリがなければ何もしない．
    fn checkpoint(&self, storage: &Storage, manager: &TransactionManager) -> Result<(), Error> {
        let Some(dir) = &self.data_dir else {
            return Ok(());
        };
        for heap in storage.tables.values() {
            heap.flush()?;
        }
        for pool in storage.indexes.values() {
            pool.flush_all()?;
        }
        manager.save(&dir.join(TRANSACTIONS_FILE))?;
        if let Some(mut wal) = self.lock_wal() {
            let lsn = wal.append(&WalRecord::Checkpoint)?;
            wal.flush_to(lsn)?;
        }
        Ok(())
    }

    fn create_table(
        &self,
        storage: &mut Storage,
        create: CreateTable,
        manager: &TransactionManager,
    ) -> Result<StatementResult, Error> {
        let schema = table_schema(create)?;
        let name = schema.name.clone();
        storage.catalog.create_table(schema)?;
        let heap = self.heap_file(&name, true)?;
        storage.tables.insert(name.clone(), heap);
        let schema = storage.catalog.table(&name)?;
        for index in storage.catalog.indexes_of(&name) {
            let pool = self.index_pool(&index.name, true)?;
            AnyIndex::create(&schema.columns[index.column].data_type, &pool)?;
            storage.indexes.insert(index.name.clone(), pool);
        }
        self.save_catalog(&storage.catalog)?;
        self.checkpoint(storage, manager)?;
        Ok(StatementResult::CreateTable)
    }

    fn drop_table(
        &self,
        storage: &mut Storage,
        drop: &DropTable,
        manager: &TransactionManager,
    ) -> Result<StatementResult, Error> {
        let index_names: Vec<String> = storage
            .catalog
            .indexes_of(&drop.name)
            .iter()
            .map(|index| index.name.clone())
            .collect();
        storage.catalog.drop_table(&drop.name)?;
        storage.tables.remove(&drop.name);
        for name in &index_names {
            storage.indexes.remove(name);
        }
        self.save_catalog(&storage.catalog)?;
        self.remove_file(&drop.name, "heap")?;
        for name in &index_names {
            self.remove_file(name, "index")?;
        }
        self.checkpoint(storage, manager)?;
        Ok(StatementResult::DropTable)
    }

    /// インデックスを作り，表の今の行をすべて入れる．行を入れられなければ，インデックスを作らない．
    fn create_index(
        &self,
        storage: &mut Storage,
        create: &CreateIndex,
        manager: &TransactionManager,
    ) -> Result<StatementResult, Error> {
        let schema = storage.catalog.table(&create.table)?;
        let column = schema.column_index(&create.column)?;
        let data_type = schema.columns[column].data_type.clone();
        let rows = storage.tables[&create.table].rows(schema, |_| true)?;
        storage.catalog.create_index(IndexDef {
            name: create.name.clone(),
            table: create.table.clone(),
            column,
        })?;
        let pool = self.index_pool(&create.name, true)?;
        let filled = fill_index(&data_type, &pool, column, rows);
        if let Err(error) = filled {
            storage.catalog.drop_index(&create.name)?;
            drop(pool);
            self.remove_file(&create.name, "index")?;
            return Err(error);
        }
        storage.indexes.insert(create.name.clone(), pool);
        self.save_catalog(&storage.catalog)?;
        self.checkpoint(storage, manager)?;
        Ok(StatementResult::CreateIndex)
    }

    fn drop_index(
        &self,
        storage: &mut Storage,
        drop: &DropIndex,
        manager: &TransactionManager,
    ) -> Result<StatementResult, Error> {
        storage.catalog.drop_index(&drop.name)?;
        storage.indexes.remove(&drop.name);
        self.save_catalog(&storage.catalog)?;
        self.remove_file(&drop.name, "index")?;
        self.checkpoint(storage, manager)?;
        Ok(StatementResult::DropIndex)
    }

    /// データディレクトリがあれば，カタログをファイルに書く．
    fn save_catalog(&self, catalog: &Catalog) -> Result<(), Error> {
        if let Some(dir) = &self.data_dir {
            catalog.save(&dir.join(CATALOG_FILE))?;
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
            wal: Arc::clone(wal),
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
}

impl Storage {
    fn insert(
        &mut self,
        insert: Insert,
        snapshot: &Snapshot,
        manager: &TransactionManager,
    ) -> Result<StatementResult, Error> {
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
        let count = dml::insert(schema, heap, &mut indexes, snapshot, manager, new_rows)?;
        Ok(StatementResult::Insert { count })
    }

    fn update(
        &mut self,
        update: &Update,
        snapshot: &Snapshot,
        manager: &TransactionManager,
    ) -> Result<Outcome, Error> {
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
        dml::update(
            schema,
            heap,
            &mut indexes,
            snapshot,
            manager,
            &assignments,
            &filter,
        )
    }

    fn delete(
        &mut self,
        delete: &Delete,
        snapshot: &Snapshot,
        manager: &TransactionManager,
    ) -> Result<Outcome, Error> {
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
        dml::delete(schema, heap, snapshot, manager, &filter)
    }

    /// `SELECT`の名前を解決し，実行計画を作る．
    fn plan(&self, select: &Select) -> Result<PlanNode, Error> {
        let bound = bind_select(select, &self.catalog)?;
        Ok(plan(&bound, &self.catalog))
    }
}

/// `SELECT`を実行し，スナップショットから見える行を返す．
fn select_rows(
    storage: &Storage,
    select: &Select,
    snapshot: &Snapshot,
    manager: &TransactionManager,
) -> Result<StatementResult, Error> {
    let plan = storage.plan(select)?;
    let columns = plan.columns().to_vec();
    let context = BuildContext {
        tables: &storage.tables,
        indexes: &storage.indexes,
        catalog: &storage.catalog,
        snapshot,
        manager,
    };
    let mut executor = build(&plan, &context)?;
    let rows = collect_rows(executor.as_mut())?;
    Ok(StatementResult::Rows(QueryResult { columns, rows }))
}

fn explain_plan(storage: &Storage, select: &Select) -> Result<StatementResult, Error> {
    let plan = storage.plan(select)?;
    let rows = explain(&plan)
        .into_iter()
        .map(|line| vec![Value::Varchar(line)])
        .collect();
    Ok(StatementResult::Rows(QueryResult {
        columns: vec![QUERY_PLAN_COLUMN.to_string()],
        rows,
    }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::txn::TxnStatus;

    #[test]
    fn dropping_a_session_in_a_transaction_aborts_the_transaction() {
        let database = Arc::new(Database::new());
        let mut session = Session::new(Arc::clone(&database));
        session.execute("START TRANSACTION").unwrap();
        let SessionState::InTransaction(txn) = &session.state else {
            panic!("expected a transaction: {:?}", session.state);
        };
        let xid = txn.xid();
        drop(session);
        assert_eq!(database.read_transactions().status(xid), TxnStatus::Aborted);
    }
}
