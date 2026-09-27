use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use ferrodb::{Database, Session, SqlState, StatementResult, TransactionStatus, Value};

/// 表`T`(`ID`が1，`N`が0の1行)を作ったデータベースと，それを共有する2つのセッション．
fn two_sessions(database: Database) -> (Session, Session) {
    let database = Arc::new(database);
    let mut a = Session::new(Arc::clone(&database));
    a.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER)")
        .unwrap();
    a.execute("INSERT INTO t VALUES (1, 0)").unwrap();
    (a, Session::new(database))
}

fn n(session: &mut Session) -> Vec<Value> {
    match session.execute("SELECT n FROM t").unwrap() {
        StatementResult::Rows(result) => {
            result.rows.into_iter().map(|row| row[0].clone()).collect()
        }
        other => panic!("expected rows: {other:?}"),
    }
}

/// ほかのスレッドの文が終わらずに待っていることを確かめるまでの時間．
const WAIT: Duration = Duration::from_millis(100);

#[test]
fn read_committed_sees_changes_committed_by_others_in_the_next_statement() {
    let (mut a, mut b) = two_sessions(Database::new());
    b.execute("START TRANSACTION").unwrap();
    assert_eq!(n(&mut b), vec![Value::Integer(0)]);
    a.execute("UPDATE t SET n = 5").unwrap();
    assert_eq!(n(&mut b), vec![Value::Integer(5)]);
}

#[test]
fn repeatable_read_sees_the_first_snapshot_until_the_end() {
    let (mut a, mut b) = two_sessions(Database::new());
    b.execute("START TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .unwrap();
    assert_eq!(n(&mut b), vec![Value::Integer(0)]);
    a.execute("UPDATE t SET n = 5").unwrap();
    assert_eq!(n(&mut b), vec![Value::Integer(0)]);
    b.execute("COMMIT").unwrap();
    assert_eq!(n(&mut b), vec![Value::Integer(5)]);
}

#[test]
fn serializable_is_not_supported() {
    let (mut a, _) = two_sessions(Database::new());
    let error = a
        .execute("START TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .unwrap_err();
    assert_eq!(error.sqlstate(), SqlState::FeatureNotSupported);
    assert_eq!(a.transaction_status(), TransactionStatus::Idle);
}

#[test]
fn read_committed_update_waits_and_changes_the_latest_version() {
    let (mut a, mut b) = two_sessions(Database::new());
    a.execute("START TRANSACTION").unwrap();
    a.execute("UPDATE t SET n = n + 1").unwrap();
    thread::scope(|scope| {
        let waiting = scope.spawn(|| b.execute("UPDATE t SET n = n + 10"));
        thread::sleep(WAIT);
        assert!(!waiting.is_finished());
        a.execute("COMMIT").unwrap();
        assert_eq!(
            waiting.join().unwrap(),
            Ok(StatementResult::Update { count: 1 })
        );
    });
    assert_eq!(n(&mut a), vec![Value::Integer(11)]);
}

#[test]
fn repeatable_read_update_fails_when_the_other_transaction_commits() {
    let (mut a, mut b) = two_sessions(Database::new());
    a.execute("START TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .unwrap();
    a.execute("UPDATE t SET n = n + 1").unwrap();
    let snapshot_taken = Barrier::new(2);
    thread::scope(|scope| {
        let waiting = scope.spawn(|| {
            b.execute("START TRANSACTION ISOLATION LEVEL REPEATABLE READ")
                .unwrap();
            n(&mut b);
            snapshot_taken.wait();
            let result = b.execute("UPDATE t SET n = n + 10");
            (result, b.transaction_status())
        });
        snapshot_taken.wait();
        thread::sleep(WAIT);
        assert!(!waiting.is_finished());
        a.execute("COMMIT").unwrap();
        let (result, status) = waiting.join().unwrap();
        assert_eq!(
            result.unwrap_err().sqlstate(),
            SqlState::SerializationFailure
        );
        assert_eq!(status, TransactionStatus::Failed);
    });
    assert_eq!(n(&mut a), vec![Value::Integer(1)]);
}

#[test]
fn update_goes_on_when_the_other_transaction_rolls_back() {
    let (mut a, mut b) = two_sessions(Database::new());
    a.execute("START TRANSACTION").unwrap();
    a.execute("UPDATE t SET n = n + 1").unwrap();
    b.execute("START TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .unwrap();
    thread::scope(|scope| {
        let waiting = scope.spawn(|| b.execute("UPDATE t SET n = n + 10"));
        thread::sleep(WAIT);
        assert!(!waiting.is_finished());
        a.execute("ROLLBACK").unwrap();
        assert_eq!(
            waiting.join().unwrap(),
            Ok(StatementResult::Update { count: 1 })
        );
    });
    b.execute("COMMIT").unwrap();
    assert_eq!(n(&mut a), vec![Value::Integer(10)]);
}

#[test]
fn waiting_longer_than_the_lock_timeout_is_an_error() {
    let database = Database::new().with_lock_timeout(Duration::from_millis(50));
    let (mut a, mut b) = two_sessions(database);
    a.execute("START TRANSACTION").unwrap();
    a.execute("DELETE FROM t").unwrap();
    let error = b.execute("UPDATE t SET n = 1").unwrap_err();
    assert_eq!(error.sqlstate(), SqlState::LockNotAvailable);
    assert_eq!(error.message(), "canceling statement due to lock timeout");
}
