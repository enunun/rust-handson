use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use ferrodb::{Database, Session, StatementResult, Value};

/// 子プロセスに，使うデータディレクトリを渡す環境変数．
const CHILD_DIR: &str = "FERRODB_CRASH_DIR";

/// 環境変数`CHILD_DIR`があれば子プロセスとして，そのデータディレクトリで`statements`を実行し，
/// 後片付けをせずにプロセスを強制終了する．なければ，一時ディレクトリを作り，同じテストを
/// 子プロセスとして起動して終わるのを待ち，そのディレクトリを返す．
fn crash_after(test: &str, statements: &[&str]) -> tempfile::TempDir {
    if let Ok(dir) = std::env::var(CHILD_DIR) {
        let mut db = Session::new(Arc::new(Database::open(Path::new(&dir)).unwrap()));
        for sql in statements {
            db.execute(sql).unwrap();
        }
        std::process::abort();
    }
    let dir = tempfile::tempdir().unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([test, "--exact", "--nocapture"])
        .env(CHILD_DIR, dir.path())
        .status()
        .unwrap();
    assert!(!status.success());
    dir
}

fn ids(db: &mut Session) -> Vec<Value> {
    match db.execute("SELECT id FROM t ORDER BY id").unwrap() {
        StatementResult::Rows(result) => {
            result.rows.into_iter().map(|row| row[0].clone()).collect()
        }
        other => panic!("not a query: {other:?}"),
    }
}

fn integers(values: &[i32]) -> Vec<Value> {
    values.iter().map(|&n| Value::Integer(n)).collect()
}

#[test]
fn committed_rows_survive_a_crash() {
    let dir = crash_after(
        "committed_rows_survive_a_crash",
        &[
            "CREATE TABLE t (id INTEGER PRIMARY KEY)",
            "INSERT INTO t VALUES (1)",
            "INSERT INTO t VALUES (2)",
            "START TRANSACTION",
            "INSERT INTO t VALUES (3)",
        ],
    );
    let mut db = Session::new(Arc::new(Database::open(dir.path()).unwrap()));
    assert_eq!(ids(&mut db), integers(&[1, 2]));
    db.execute("INSERT INTO t VALUES (3)").unwrap();
    assert_eq!(ids(&mut db), integers(&[1, 2, 3]));
}

#[test]
fn changes_after_a_checkpoint_are_redone() {
    let dir = crash_after(
        "changes_after_a_checkpoint_are_redone",
        &[
            "CREATE TABLE t (id INTEGER PRIMARY KEY)",
            "INSERT INTO t VALUES (1), (2)",
            "CHECKPOINT",
            "DELETE FROM t WHERE id = 1",
            "INSERT INTO t VALUES (5)",
        ],
    );
    let mut db = Session::new(Arc::new(Database::open(dir.path()).unwrap()));
    assert_eq!(ids(&mut db), integers(&[2, 5]));
    let error = db.execute("INSERT INTO t VALUES (5)").unwrap_err();
    assert_eq!(error.sqlstate().code(), "23505");
}

#[test]
fn torn_record_at_the_end_of_the_log_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Session::new(Arc::new(Database::open(dir.path()).unwrap()));
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)")
        .unwrap();
    db.execute("INSERT INTO t VALUES (1)").unwrap();
    drop(db);
    let wal = dir.path().join("wal");
    let mut bytes = std::fs::read(&wal).unwrap();
    bytes.extend_from_slice(&[100, 0, 0, 0, 1, 2, 3]);
    std::fs::write(&wal, bytes).unwrap();

    let mut db = Session::new(Arc::new(Database::open(dir.path()).unwrap()));
    assert_eq!(ids(&mut db), integers(&[1]));
    db.execute("INSERT INTO t VALUES (2)").unwrap();
    drop(db);
    let mut db = Session::new(Arc::new(Database::open(dir.path()).unwrap()));
    assert_eq!(ids(&mut db), integers(&[1, 2]));
}

#[test]
fn checkpoint_statement_returns_its_tag() {
    let mut db = Session::new(Arc::new(Database::new()));
    assert_eq!(db.execute("CHECKPOINT"), Ok(StatementResult::Checkpoint));
}
