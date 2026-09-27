use ferrodb::{Database, SqlState, StatementResult, Value};

fn database() -> Database {
    let mut db = Database::new();
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR(20), age INTEGER)")
        .unwrap();
    db.execute("INSERT INTO users VALUES (1, 'alice', 30), (2, 'bob', NULL), (3, 'carol', 25)")
        .unwrap();
    db
}

fn rows(db: &mut Database, sql: &str) -> Vec<Vec<Value>> {
    match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result.rows,
        other => panic!("not a query: {other:?}"),
    }
}

fn varchar(s: &str) -> Value {
    Value::Varchar(s.to_string())
}

#[test]
fn update_changes_matching_rows_and_counts_them() {
    let mut db = database();
    assert_eq!(
        db.execute("UPDATE users SET age = age + 1, name = 'bobby' WHERE id = 2 OR id = 3"),
        Ok(StatementResult::Update { count: 2 })
    );
    assert_eq!(
        rows(&mut db, "SELECT * FROM users"),
        vec![
            vec![Value::Integer(1), varchar("alice"), Value::Integer(30)],
            vec![Value::Integer(2), varchar("bobby"), Value::Null],
            vec![Value::Integer(3), varchar("bobby"), Value::Integer(26)],
        ]
    );
}

#[test]
fn update_without_where_changes_every_row() {
    let mut db = database();
    assert_eq!(
        db.execute("UPDATE users SET age = 0"),
        Ok(StatementResult::Update { count: 3 })
    );
    assert_eq!(
        rows(&mut db, "SELECT age FROM users WHERE age = 0").len(),
        3
    );
}

#[test]
fn update_matching_no_rows_counts_zero() {
    let mut db = database();
    assert_eq!(
        db.execute("UPDATE users SET age = 0 WHERE id = 9"),
        Ok(StatementResult::Update { count: 0 })
    );
}

#[test]
fn assignments_use_the_values_before_the_update() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER, b INTEGER)").unwrap();
    db.execute("INSERT INTO t VALUES (1, 2)").unwrap();
    db.execute("UPDATE t SET a = b, b = a").unwrap();
    assert_eq!(
        rows(&mut db, "SELECT * FROM t"),
        vec![vec![Value::Integer(2), Value::Integer(1)]]
    );
}

#[test]
fn assigned_value_must_fit_the_column_type() {
    let mut db = database();
    let err = db.execute("UPDATE users SET age = TRUE").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::DatatypeMismatch);
    assert_eq!(
        err.to_string(),
        "column \"AGE\" is of type integer but expression is of type boolean"
    );
}

#[test]
fn assigning_to_an_unknown_column_is_an_error() {
    let mut db = database();
    let err = db.execute("UPDATE users SET email = 'x'").unwrap_err();
    assert_eq!(err.sqlstate().code(), "42703");
    assert_eq!(
        err.to_string(),
        "column \"EMAIL\" of relation \"USERS\" does not exist"
    );
}

#[test]
fn assigning_to_the_same_column_twice_is_an_error() {
    let mut db = database();
    let err = db.execute("UPDATE users SET age = 1, age = 2").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::SyntaxError);
    assert_eq!(
        err.to_string(),
        "multiple assignments to same column \"AGE\""
    );
}

#[test]
fn delete_removes_matching_rows_and_counts_them() {
    let mut db = database();
    assert_eq!(
        db.execute("DELETE FROM users WHERE age > 26 OR age IS NULL"),
        Ok(StatementResult::Delete { count: 2 })
    );
    assert_eq!(
        rows(&mut db, "SELECT id FROM users"),
        vec![vec![Value::Integer(3)]]
    );
}

#[test]
fn delete_without_where_removes_every_row() {
    let mut db = database();
    assert_eq!(
        db.execute("DELETE FROM users"),
        Ok(StatementResult::Delete { count: 3 })
    );
    assert_eq!(rows(&mut db, "SELECT * FROM users").len(), 0);
}

#[test]
fn error_in_the_middle_of_a_statement_changes_no_rows() {
    let mut db = database();
    let err = db
        .execute("UPDATE users SET age = 0 WHERE 10 / (id - 2) > 0")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::DivisionByZero);
    let err = db
        .execute("DELETE FROM users WHERE 10 / (id - 2) > 0")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::DivisionByZero);
    assert_eq!(
        rows(&mut db, "SELECT age FROM users"),
        vec![
            vec![Value::Integer(30)],
            vec![Value::Null],
            vec![Value::Integer(25)],
        ]
    );
}
