use ferrodb::{Database, SqlState, StatementResult, Value};

fn database() -> Database {
    let mut db = Database::new();
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR(20), age INTEGER)")
        .unwrap();
    db.execute(
        "INSERT INTO users VALUES (1, 'alice', 30), (2, 'bob', NULL), (3, 'carol', 25), (4, 'dave', 30)",
    )
    .unwrap();
    db
}

fn rows(db: &mut Database, sql: &str) -> Vec<Vec<Value>> {
    match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result.rows,
        other => panic!("not a query: {other:?}"),
    }
}

fn ids(db: &mut Database, sql: &str) -> Vec<i32> {
    rows(db, sql)
        .into_iter()
        .map(|row| match row[0] {
            Value::Integer(id) => id,
            ref other => panic!("not an integer: {other:?}"),
        })
        .collect()
}

fn varchar(s: &str) -> Value {
    Value::Varchar(s.to_string())
}

#[test]
fn orders_by_a_column_descending_and_limits_the_rows() {
    let mut db = database();
    assert_eq!(
        rows(
            &mut db,
            "SELECT name FROM users ORDER BY name DESC OFFSET 1 ROWS FETCH FIRST 2 ROWS ONLY"
        ),
        vec![vec![varchar("carol")], vec![varchar("bob")]]
    );
}

#[test]
fn null_is_last_ascending_and_first_descending() {
    let mut db = database();
    assert_eq!(
        ids(&mut db, "SELECT id FROM users ORDER BY age, id"),
        vec![3, 1, 4, 2]
    );
    assert_eq!(
        ids(&mut db, "SELECT id FROM users ORDER BY age DESC, id"),
        vec![2, 1, 4, 3]
    );
}

#[test]
fn nulls_first_and_nulls_last() {
    let mut db = database();
    assert_eq!(
        ids(&mut db, "SELECT id FROM users ORDER BY age NULLS FIRST, id"),
        vec![2, 3, 1, 4]
    );
    assert_eq!(
        ids(
            &mut db,
            "SELECT id FROM users ORDER BY age DESC NULLS LAST, id"
        ),
        vec![1, 4, 3, 2]
    );
}

#[test]
fn later_keys_order_rows_with_equal_earlier_keys() {
    let mut db = database();
    assert_eq!(
        ids(
            &mut db,
            "SELECT id FROM users WHERE age = 30 ORDER BY age, id DESC"
        ),
        vec![4, 1]
    );
}

#[test]
fn order_by_can_use_an_output_alias() {
    let mut db = database();
    assert_eq!(
        ids(&mut db, "SELECT id, 0 - id AS rev FROM users ORDER BY rev"),
        vec![4, 3, 2, 1]
    );
}

#[test]
fn order_by_can_use_a_column_that_is_not_selected() {
    let mut db = database();
    assert_eq!(
        rows(
            &mut db,
            "SELECT name FROM users ORDER BY id DESC FETCH FIRST 1 ROWS ONLY"
        ),
        vec![vec![varchar("dave")]]
    );
}

#[test]
fn offset_beyond_the_rows_returns_no_rows() {
    let mut db = database();
    assert_eq!(rows(&mut db, "SELECT * FROM users OFFSET 10 ROWS").len(), 0);
    assert_eq!(
        rows(&mut db, "SELECT * FROM users FETCH FIRST 0 ROWS ONLY").len(),
        0
    );
}

#[test]
fn distinct_removes_duplicate_rows() {
    let mut db = database();
    db.execute("INSERT INTO users VALUES (5, 'eve', NULL)")
        .unwrap();
    assert_eq!(
        rows(&mut db, "SELECT DISTINCT age FROM users ORDER BY age"),
        vec![
            vec![Value::Integer(25)],
            vec![Value::Integer(30)],
            vec![Value::Null],
        ]
    );
}

#[test]
fn distinct_is_applied_before_offset_and_fetch() {
    let mut db = database();
    assert_eq!(
        rows(
            &mut db,
            "SELECT DISTINCT age FROM users ORDER BY age FETCH FIRST 2 ROWS ONLY"
        ),
        vec![vec![Value::Integer(25)], vec![Value::Integer(30)]]
    );
}

#[test]
fn distinct_cannot_order_by_an_unselected_expression() {
    let mut db = database();
    let err = db
        .execute("SELECT DISTINCT name FROM users ORDER BY id")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::InvalidColumnReference);
    assert_eq!(err.sqlstate().code(), "42P10");
    assert_eq!(
        err.to_string(),
        "for SELECT DISTINCT, ORDER BY expressions must appear in select list"
    );
}

#[test]
fn ordering_by_an_unknown_column_is_an_error() {
    let mut db = database();
    let err = db
        .execute("SELECT * FROM users ORDER BY email")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedColumn);
}
