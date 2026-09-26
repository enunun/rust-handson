use ferrodb::{Database, QueryResult, SqlState, StatementResult, Value};

fn rows(db: &mut Database, sql: &str) -> QueryResult {
    match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result,
        other => panic!("not a query: {other:?}"),
    }
}

fn varchar(s: &str) -> Value {
    Value::Varchar(s.to_string())
}

#[test]
fn creates_a_table_inserts_rows_and_selects_them() {
    let mut db = Database::new();
    assert_eq!(
        db.execute("CREATE TABLE users (id INTEGER, name VARCHAR(20))"),
        Ok(StatementResult::CreateTable)
    );
    assert_eq!(
        db.execute("INSERT INTO users VALUES (1, 'alice'), (2, 'bob')"),
        Ok(StatementResult::Insert { count: 2 })
    );
    assert_eq!(
        rows(&mut db, "SELECT * FROM users"),
        QueryResult {
            columns: vec!["ID".to_string(), "NAME".to_string()],
            rows: vec![
                vec![Value::Integer(1), varchar("alice")],
                vec![Value::Integer(2), varchar("bob")],
            ]
        }
    );
}

#[test]
fn a_new_table_has_no_rows() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a BOOLEAN)").unwrap();
    assert_eq!(
        rows(&mut db, "SELECT * FROM t").rows,
        Vec::<Vec<Value>>::new()
    );
}

#[test]
fn unquoted_names_are_case_insensitive() {
    let mut db = Database::new();
    db.execute("CREATE TABLE Users (Id INTEGER)").unwrap();
    db.execute("insert into USERS values (1)").unwrap();
    assert_eq!(rows(&mut db, "select * from users").columns, vec!["ID"]);
}

#[test]
fn quoted_names_are_case_sensitive() {
    let mut db = Database::new();
    db.execute("CREATE TABLE \"users\" (\"id\" INTEGER)")
        .unwrap();
    assert_eq!(rows(&mut db, "SELECT * FROM \"users\"").columns, vec!["id"]);
    let err = db.execute("SELECT * FROM users").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedTable);
    assert_eq!(err.to_string(), "relation \"USERS\" does not exist");
}

#[test]
fn inserts_into_named_columns_and_fills_the_rest_with_null() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER, b VARCHAR(5), c BOOLEAN)")
        .unwrap();
    db.execute("INSERT INTO t (c, a) VALUES (TRUE, 1)").unwrap();
    assert_eq!(
        rows(&mut db, "SELECT * FROM t").rows,
        vec![vec![Value::Integer(1), Value::Null, Value::Boolean(true)]]
    );
}

#[test]
fn bigint_column_stores_integers_as_bigint() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a BIGINT, b INTEGER)").unwrap();
    db.execute("INSERT INTO t VALUES (1, 2147483648 - 1)")
        .unwrap();
    assert_eq!(
        rows(&mut db, "SELECT * FROM t").rows,
        vec![vec![Value::BigInt(1), Value::Integer(2147483647)]]
    );
}

#[test]
fn creating_an_existing_table_is_an_error() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    let err = db.execute("CREATE TABLE T (b INTEGER)").unwrap_err();
    assert_eq!(err.sqlstate().code(), "42P07");
    assert_eq!(err.to_string(), "relation \"T\" already exists");
}

#[test]
fn inserting_into_an_unknown_table_is_an_error() {
    let mut db = Database::new();
    let err = db.execute("INSERT INTO t VALUES (1)").unwrap_err();
    assert_eq!(err.sqlstate().code(), "42P01");
}

#[test]
fn inserting_into_an_unknown_column_is_an_error() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    let err = db.execute("INSERT INTO t (b) VALUES (1)").unwrap_err();
    assert_eq!(err.sqlstate().code(), "42703");
    assert_eq!(
        err.to_string(),
        "column \"B\" of relation \"T\" does not exist"
    );
}

#[test]
fn inserting_a_value_of_another_type_is_an_error() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    let err = db.execute("INSERT INTO t VALUES (TRUE)").unwrap_err();
    assert_eq!(err.sqlstate().code(), "42804");
    assert_eq!(
        err.to_string(),
        "column \"A\" is of type integer but expression is of type boolean"
    );
}

#[test]
fn inserting_a_too_long_string_is_an_error() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a VARCHAR(3))").unwrap();
    let err = db.execute("INSERT INTO t VALUES ('abcd')").unwrap_err();
    assert_eq!(err.sqlstate().code(), "22001");
    assert_eq!(
        err.to_string(),
        "value too long for type character varying(3)"
    );
}

#[test]
fn inserting_a_bigint_beyond_integer_into_an_integer_column_is_an_error() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    let err = db.execute("INSERT INTO t VALUES (2147483648)").unwrap_err();
    assert_eq!(err.sqlstate().code(), "22003");
}

#[test]
fn number_of_values_must_match_the_columns() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER, b INTEGER)").unwrap();
    let err = db.execute("INSERT INTO t VALUES (1, 2, 3)").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::SyntaxError);
    assert_eq!(
        err.to_string(),
        "INSERT has more expressions than target columns"
    );
    let err = db.execute("INSERT INTO t VALUES (1)").unwrap_err();
    assert_eq!(
        err.to_string(),
        "INSERT has more target columns than expressions"
    );
}

#[test]
fn a_failing_insert_adds_no_rows() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    db.execute("INSERT INTO t VALUES (1), (TRUE)").unwrap_err();
    assert_eq!(
        rows(&mut db, "SELECT * FROM t").rows,
        Vec::<Vec<Value>>::new()
    );
}

#[test]
fn dropped_table_is_gone_and_can_be_created_again() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    db.execute("INSERT INTO t VALUES (1)").unwrap();
    assert_eq!(db.execute("DROP TABLE t"), Ok(StatementResult::DropTable));
    let err = db.execute("SELECT * FROM t").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedTable);
    db.execute("CREATE TABLE t (b BOOLEAN)").unwrap();
    assert_eq!(rows(&mut db, "SELECT * FROM t").columns, vec!["B"]);
    assert_eq!(rows(&mut db, "SELECT * FROM t").rows.len(), 0);
}

#[test]
fn dropping_an_unknown_table_is_an_error() {
    let mut db = Database::new();
    let err = db.execute("DROP TABLE t").unwrap_err();
    assert_eq!(err.sqlstate().code(), "42P01");
    assert_eq!(err.to_string(), "table \"T\" does not exist");
}
