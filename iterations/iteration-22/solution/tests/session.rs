use std::sync::Arc;
use std::thread;

use ferrodb::{Database, Session, StatementResult, TransactionStatus, Value};

fn count(session: &mut Session) -> Value {
    match session.execute("SELECT COUNT(*) FROM t").unwrap() {
        StatementResult::Rows(result) => result.rows[0][0].clone(),
        other => panic!("expected rows: {other:?}"),
    }
}

/// 表`T`を作ったデータベースと，それを共有する2つのセッション．
fn two_sessions() -> (Session, Session) {
    let database = Arc::new(Database::new());
    let mut a = Session::new(Arc::clone(&database));
    a.execute("CREATE TABLE t (a INTEGER)").unwrap();
    (a, Session::new(database))
}

#[test]
fn sessions_have_their_own_transaction_status() {
    let (mut a, b) = two_sessions();
    a.execute("START TRANSACTION").unwrap();
    assert_eq!(a.transaction_status(), TransactionStatus::InTransaction);
    assert_eq!(b.transaction_status(), TransactionStatus::Idle);
}

#[test]
fn uncommitted_rows_of_another_session_are_invisible() {
    let (mut a, mut b) = two_sessions();
    a.execute("START TRANSACTION").unwrap();
    a.execute("INSERT INTO t VALUES (1)").unwrap();
    assert_eq!(count(&mut a), Value::BigInt(1));
    assert_eq!(count(&mut b), Value::BigInt(0));
    a.execute("COMMIT").unwrap();
    assert_eq!(count(&mut b), Value::BigInt(1));
}

#[test]
fn sessions_in_several_threads_insert_rows_at_the_same_time() {
    let dir = tempfile::tempdir().unwrap();
    let database = Arc::new(Database::open(dir.path()).unwrap());
    Session::new(Arc::clone(&database))
        .execute("CREATE TABLE t (a INTEGER)")
        .unwrap();
    thread::scope(|scope| {
        for n in 0..4 {
            let mut session = Session::new(Arc::clone(&database));
            scope.spawn(move || {
                for i in 0..25 {
                    let sql = format!("INSERT INTO t VALUES ({})", n * 100 + i);
                    session.execute(&sql).unwrap();
                }
            });
        }
    });
    drop(database);
    let mut session = Session::new(Arc::new(Database::open(dir.path()).unwrap()));
    assert_eq!(count(&mut session), Value::BigInt(100));
}
