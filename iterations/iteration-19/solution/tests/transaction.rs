use ferrodb::{Database, SqlState, StatementResult, TransactionStatus, Value};

fn database() -> Database {
    let mut db = Database::new();
    db.execute("CREATE TABLE emp (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL)")
        .unwrap();
    db.execute("INSERT INTO emp VALUES (1, 'alice'), (2, 'bob'), (3, 'carol'), (4, 'dave')")
        .unwrap();
    db
}

fn count(db: &mut Database) -> Value {
    match db.execute("SELECT COUNT(*) FROM emp").unwrap() {
        StatementResult::Rows(result) => result.rows[0][0].clone(),
        other => panic!("not a query: {other:?}"),
    }
}

fn names(db: &mut Database) -> Vec<Value> {
    match db.execute("SELECT name FROM emp ORDER BY id").unwrap() {
        StatementResult::Rows(result) => {
            result.rows.into_iter().map(|row| row[0].clone()).collect()
        }
        other => panic!("not a query: {other:?}"),
    }
}

fn varchars(names: &[&str]) -> Vec<Value> {
    names
        .iter()
        .map(|name| Value::Varchar(name.to_string()))
        .collect()
}

#[test]
fn rolled_back_changes_disappear() {
    let mut db = database();
    assert_eq!(
        db.execute("START TRANSACTION"),
        Ok(StatementResult::StartTransaction)
    );
    assert_eq!(db.transaction_status(), TransactionStatus::InTransaction);
    assert_eq!(
        db.execute("DELETE FROM emp"),
        Ok(StatementResult::Delete { count: 4 })
    );
    assert_eq!(count(&mut db), Value::BigInt(0));
    assert_eq!(db.execute("ROLLBACK"), Ok(StatementResult::Rollback));
    assert_eq!(db.transaction_status(), TransactionStatus::Idle);
    assert_eq!(count(&mut db), Value::BigInt(4));
}

#[test]
fn committed_changes_remain() {
    let mut db = database();
    db.execute("START TRANSACTION").unwrap();
    db.execute("UPDATE emp SET name = 'bobby' WHERE id = 2")
        .unwrap();
    db.execute("INSERT INTO emp VALUES (5, 'eve')").unwrap();
    assert_eq!(db.execute("COMMIT"), Ok(StatementResult::Commit));
    assert_eq!(
        names(&mut db),
        varchars(&["alice", "bobby", "carol", "dave", "eve"])
    );
}

#[test]
fn rows_updated_twice_return_to_the_first_version_on_rollback() {
    let mut db = database();
    db.execute("START TRANSACTION").unwrap();
    db.execute("UPDATE emp SET name = 'x' WHERE id = 1")
        .unwrap();
    db.execute("UPDATE emp SET name = 'y' WHERE id = 1")
        .unwrap();
    assert_eq!(names(&mut db)[0], Value::Varchar("y".to_string()));
    db.execute("ROLLBACK").unwrap();
    assert_eq!(names(&mut db), varchars(&["alice", "bob", "carol", "dave"]));
}

#[test]
fn error_fails_the_transaction_until_it_ends() {
    let mut db = database();
    db.execute("START TRANSACTION").unwrap();
    db.execute("DELETE FROM emp WHERE id = 1").unwrap();
    let error = db.execute("INSERT INTO emp VALUES (2, 'x')").unwrap_err();
    assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
    assert_eq!(db.transaction_status(), TransactionStatus::Failed);
    let error = db.execute("SELECT * FROM emp").unwrap_err();
    assert_eq!(error.sqlstate().code(), "25P02");
    assert_eq!(
        error.message(),
        "current transaction is aborted, commands ignored until end of transaction block"
    );
    assert_eq!(db.execute("COMMIT"), Ok(StatementResult::Rollback));
    assert_eq!(db.transaction_status(), TransactionStatus::Idle);
    assert_eq!(count(&mut db), Value::BigInt(4));
}

#[test]
fn syntax_error_also_fails_the_transaction() {
    let mut db = database();
    db.execute("START TRANSACTION").unwrap();
    db.execute("SELEC 1").unwrap_err();
    assert_eq!(db.transaction_status(), TransactionStatus::Failed);
    assert_eq!(db.execute("ROLLBACK"), Ok(StatementResult::Rollback));
}

#[test]
fn start_inside_a_transaction_is_an_error_that_keeps_it_going() {
    let mut db = database();
    db.execute("START TRANSACTION").unwrap();
    let error = db.execute("START TRANSACTION").unwrap_err();
    assert_eq!(error.sqlstate().code(), "25001");
    assert_eq!(
        error.message(),
        "there is already a transaction in progress"
    );
    assert_eq!(db.transaction_status(), TransactionStatus::InTransaction);
    db.execute("DELETE FROM emp WHERE id = 4").unwrap();
    db.execute("COMMIT").unwrap();
    assert_eq!(count(&mut db), Value::BigInt(3));
}

#[test]
fn table_statements_cannot_run_inside_a_transaction() {
    let mut db = database();
    db.execute("START TRANSACTION").unwrap();
    let error = db.execute("CREATE TABLE t (a INTEGER)").unwrap_err();
    assert_eq!(error.sqlstate().code(), "25001");
    assert_eq!(
        error.message(),
        "CREATE TABLE cannot run inside a transaction block"
    );
    assert_eq!(db.transaction_status(), TransactionStatus::Failed);
    db.execute("ROLLBACK").unwrap();
    assert_eq!(
        db.execute("CREATE TABLE t (a INTEGER)"),
        Ok(StatementResult::CreateTable)
    );
}

#[test]
fn commit_and_rollback_outside_a_transaction_do_nothing() {
    let mut db = database();
    assert_eq!(db.execute("COMMIT"), Ok(StatementResult::Commit));
    assert_eq!(db.execute("ROLLBACK"), Ok(StatementResult::Rollback));
    assert_eq!(db.transaction_status(), TransactionStatus::Idle);
}

#[test]
fn statement_outside_a_transaction_that_fails_changes_nothing() {
    let mut db = database();
    db.execute("UPDATE emp SET id = 10 / (id - 3)").unwrap_err();
    assert_eq!(db.transaction_status(), TransactionStatus::Idle);
    assert_eq!(names(&mut db), varchars(&["alice", "bob", "carol", "dave"]));
}

#[test]
fn only_committed_transactions_remain_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    db.execute("CREATE TABLE emp (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL)")
        .unwrap();
    db.execute("INSERT INTO emp VALUES (1, 'alice')").unwrap();
    db.execute("START TRANSACTION").unwrap();
    db.execute("INSERT INTO emp VALUES (2, 'bob')").unwrap();
    db.execute("UPDATE emp SET name = 'x' WHERE id = 1")
        .unwrap();
    drop(db);

    let mut db = Database::open(dir.path()).unwrap();
    assert_eq!(names(&mut db), varchars(&["alice"]));
    db.execute("INSERT INTO emp VALUES (2, 'carol')").unwrap();
    assert_eq!(names(&mut db), varchars(&["alice", "carol"]));
}

#[test]
fn index_scan_sees_the_same_versions_as_a_seq_scan() {
    let mut db = database();
    db.execute("START TRANSACTION").unwrap();
    db.execute("UPDATE emp SET id = id + 10 WHERE id <= 2")
        .unwrap();
    let sql = "SELECT name FROM emp WHERE id >= 2 ORDER BY id";
    let inside = match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result.rows,
        other => panic!("not a query: {other:?}"),
    };
    assert_eq!(
        inside,
        vec![
            varchars(&["carol"]),
            varchars(&["dave"]),
            varchars(&["alice"]),
            varchars(&["bob"]),
        ]
    );
    db.execute("ROLLBACK").unwrap();
    let after = match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result.rows,
        other => panic!("not a query: {other:?}"),
    };
    assert_eq!(
        after,
        vec![
            varchars(&["bob"]),
            varchars(&["carol"]),
            varchars(&["dave"])
        ]
    );
}
