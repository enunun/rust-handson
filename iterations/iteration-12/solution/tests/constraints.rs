use ferrodb::{Database, SqlState, StatementResult, Value};

fn database() -> Database {
    let mut db = Database::new();
    db.execute(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL, email VARCHAR(20) UNIQUE)",
    )
    .unwrap();
    db.execute("INSERT INTO users VALUES (1, 'alice', 'a@example.com'), (2, 'bob', NULL)")
        .unwrap();
    db
}

fn ids(db: &mut Database) -> Vec<Vec<Value>> {
    match db.execute("SELECT id FROM users").unwrap() {
        StatementResult::Rows(result) => result.rows,
        other => panic!("not a query: {other:?}"),
    }
}

#[test]
fn duplicate_primary_key_is_rejected_and_no_row_is_inserted() {
    let mut db = database();
    let err = db
        .execute("INSERT INTO users VALUES (3, 'carol', NULL), (1, 'dave', NULL)")
        .unwrap_err();
    assert_eq!(err.sqlstate().code(), "23505");
    assert_eq!(
        err.to_string(),
        "duplicate key value violates unique constraint \"USERS_PKEY\""
    );
    assert_eq!(
        ids(&mut db),
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
}

#[test]
fn unique_constraint_is_named_after_the_table_and_the_column() {
    let mut db = database();
    let err = db
        .execute("INSERT INTO users VALUES (3, 'carol', 'a@example.com')")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UniqueViolation);
    assert_eq!(
        err.to_string(),
        "duplicate key value violates unique constraint \"USERS_EMAIL_KEY\""
    );
}

#[test]
fn unique_column_accepts_many_nulls() {
    let mut db = database();
    assert_eq!(
        db.execute("INSERT INTO users VALUES (3, 'carol', NULL)"),
        Ok(StatementResult::Insert { count: 1 })
    );
}

#[test]
fn null_in_a_not_null_column_is_rejected() {
    let mut db = database();
    let err = db
        .execute("INSERT INTO users VALUES (3, NULL, NULL)")
        .unwrap_err();
    assert_eq!(err.sqlstate().code(), "23502");
    assert_eq!(
        err.to_string(),
        "null value in column \"NAME\" of relation \"USERS\" violates not-null constraint"
    );
    let err = db.execute("INSERT INTO users (id) VALUES (3)").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::NotNullViolation);
}

#[test]
fn primary_key_column_is_not_null() {
    let mut db = database();
    let err = db
        .execute("INSERT INTO users VALUES (NULL, 'carol', NULL)")
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "null value in column \"ID\" of relation \"USERS\" violates not-null constraint"
    );
}

#[test]
fn update_to_a_duplicate_value_is_rejected_and_no_row_changes() {
    let mut db = database();
    let err = db.execute("UPDATE users SET id = 1").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UniqueViolation);
    assert_eq!(
        ids(&mut db),
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
}

#[test]
fn uniqueness_is_checked_at_the_end_of_the_statement() {
    let mut db = database();
    assert_eq!(
        db.execute("UPDATE users SET id = id + 1"),
        Ok(StatementResult::Update { count: 2 })
    );
    assert_eq!(
        ids(&mut db),
        vec![vec![Value::Integer(2)], vec![Value::Integer(3)]]
    );
}

#[test]
fn a_table_has_at_most_one_primary_key() {
    let mut db = Database::new();
    let err = db
        .execute("CREATE TABLE t (a INTEGER PRIMARY KEY, b INTEGER PRIMARY KEY)")
        .unwrap_err();
    assert_eq!(err.sqlstate().code(), "42P16");
    assert_eq!(
        err.to_string(),
        "multiple primary keys for table \"T\" are not allowed"
    );
}
