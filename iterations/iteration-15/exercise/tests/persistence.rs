use std::path::Path;

use ferrodb::{Database, SqlState, StatementResult, Value};

fn rows(db: &mut Database, sql: &str) -> Vec<Vec<Value>> {
    match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result.rows,
        other => panic!("not a query: {other:?}"),
    }
}

fn open(dir: &Path) -> Database {
    Database::open(dir).unwrap()
}

#[test]
fn tables_and_rows_remain_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = open(dir.path());
    db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL)")
        .unwrap();
    db.execute("INSERT INTO users VALUES (1, 'alice'), (2, 'bob'), (3, 'carol')")
        .unwrap();
    db.execute("UPDATE users SET name = 'bobby' WHERE id = 2")
        .unwrap();
    db.execute("DELETE FROM users WHERE id = 3").unwrap();
    drop(db);

    let mut db = open(dir.path());
    assert_eq!(
        rows(&mut db, "SELECT * FROM users"),
        vec![
            vec![Value::Integer(1), Value::Varchar("alice".to_string())],
            vec![Value::Integer(2), Value::Varchar("bobby".to_string())],
        ]
    );
    let err = db
        .execute("INSERT INTO users VALUES (1, 'dave')")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UniqueViolation);
}

#[test]
fn dropped_table_is_gone_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = open(dir.path());
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    db.execute("INSERT INTO t VALUES (1)").unwrap();
    db.execute("DROP TABLE t").unwrap();
    drop(db);

    let mut db = open(dir.path());
    let err = db.execute("SELECT * FROM t").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedTable);
    db.execute("CREATE TABLE t (b BOOLEAN)").unwrap();
    assert_eq!(rows(&mut db, "SELECT * FROM t").len(), 0);
}

#[test]
fn data_directory_holds_the_catalog_and_one_heap_file_per_table() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = open(&dir.path().join("data"));
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    db.execute("INSERT INTO t VALUES (1)").unwrap();
    let mut files: Vec<String> = std::fs::read_dir(dir.path().join("data"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(files, vec!["54.heap", "catalog"]);
    let heap = std::fs::metadata(dir.path().join("data").join("54.heap")).unwrap();
    assert_eq!(heap.len(), 8192);
}

#[test]
fn database_without_a_data_directory_starts_empty() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = open(dir.path());
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    drop(db);
    let mut memory = Database::new();
    let err = memory.execute("SELECT * FROM t").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedTable);
}
