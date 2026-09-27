use ferrodb::{Database, QueryResult, SqlState, StatementResult, Value};

fn database() -> Database {
    let mut db = Database::new();
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR(20), age INTEGER)")
        .unwrap();
    db.execute("INSERT INTO users VALUES (1, 'alice', 30), (2, 'bob', NULL), (3, 'carol', 25)")
        .unwrap();
    db
}

fn query(db: &mut Database, sql: &str) -> QueryResult {
    match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result,
        other => panic!("not a query: {other:?}"),
    }
}

fn varchar(s: &str) -> Value {
    Value::Varchar(s.to_string())
}

#[test]
fn selects_columns_and_expressions_with_aliases() {
    let result = query(
        &mut database(),
        "SELECT name, id * 10 AS score FROM users WHERE id >= 2",
    );
    assert_eq!(result.columns, vec!["NAME", "SCORE"]);
    assert_eq!(
        result.rows,
        vec![
            vec![varchar("bob"), Value::Integer(20)],
            vec![varchar("carol"), Value::Integer(30)],
        ]
    );
}

#[test]
fn where_keeps_only_rows_where_the_condition_is_true() {
    let result = query(&mut database(), "SELECT id FROM users WHERE age > 26");
    assert_eq!(result.rows, vec![vec![Value::Integer(1)]]);
}

#[test]
fn where_drops_rows_where_the_condition_is_unknown() {
    let result = query(&mut database(), "SELECT id FROM users WHERE NOT age > 26");
    assert_eq!(result.rows, vec![vec![Value::Integer(3)]]);
}

#[test]
fn expression_without_alias_is_named_question_column() {
    let result = query(&mut database(), "SELECT id + 1 FROM users WHERE id = 1");
    assert_eq!(result.columns, vec!["?column?"]);
    assert_eq!(result.rows, vec![vec![Value::Integer(2)]]);
}

#[test]
fn star_and_expressions_can_be_mixed() {
    let result = query(
        &mut database(),
        "SELECT *, age IS NULL FROM users WHERE id = 2",
    );
    assert_eq!(result.columns, vec!["ID", "NAME", "AGE", "?column?"]);
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Integer(2),
            varchar("bob"),
            Value::Null,
            Value::Boolean(true)
        ]]
    );
}

#[test]
fn unknown_column_is_an_error() {
    let err = database().execute("SELECT email FROM users").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedColumn);
    assert_eq!(err.to_string(), "column \"EMAIL\" does not exist");
}

#[test]
fn where_condition_must_be_boolean() {
    let err = database()
        .execute("SELECT * FROM users WHERE id")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::DatatypeMismatch);
    assert_eq!(
        err.to_string(),
        "argument of WHERE must be type boolean, not type integer"
    );
}

#[test]
fn values_cannot_refer_to_columns() {
    let err = Database::new().execute("VALUES (a)").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedColumn);
}
