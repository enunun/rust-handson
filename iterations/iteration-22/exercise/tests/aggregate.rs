use std::sync::Arc;

use ferrodb::{Database, QueryResult, Session, SqlState, StatementResult, Value};

fn database() -> Session {
    let mut db = Session::new(Arc::new(Database::new()));
    db.execute(
        "CREATE TABLE emp (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL, dept VARCHAR(10), salary INTEGER)",
    )
    .unwrap();
    db.execute(
        "INSERT INTO emp VALUES (1, 'Sato', 'dev', 500), (2, 'Suzuki', 'dev', 450), (3, 'Tanaka', 'ops', 400), (4, 'Ito', NULL, 300)",
    )
    .unwrap();
    db
}

fn query(db: &mut Session, sql: &str) -> QueryResult {
    match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result,
        other => panic!("not a query: {other:?}"),
    }
}

fn varchar(s: &str) -> Value {
    Value::Varchar(s.to_string())
}

fn big(n: i64) -> Value {
    Value::BigInt(n)
}

#[test]
fn group_by_with_having() {
    let result = query(
        &mut database(),
        "SELECT dept, COUNT(*) AS n, SUM(salary) AS total FROM emp GROUP BY dept HAVING COUNT(*) > 1",
    );
    assert_eq!(result.columns, vec!["DEPT", "N", "TOTAL"]);
    assert_eq!(result.rows, vec![vec![varchar("dev"), big(2), big(950)]]);
}

#[test]
fn null_keys_form_one_group() {
    let result = query(
        &mut database(),
        "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY dept",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![varchar("dev"), big(2)],
            vec![varchar("ops"), big(1)],
            vec![Value::Null, big(1)],
        ]
    );
}

#[test]
fn aggregates_without_group_by_make_one_group() {
    let result = query(
        &mut database(),
        "SELECT COUNT(*), COUNT(dept), AVG(salary), MIN(name), MAX(salary) FROM emp",
    );
    assert_eq!(result.columns, vec!["COUNT", "COUNT", "AVG", "MIN", "MAX"]);
    assert_eq!(
        result.rows,
        vec![vec![
            big(4),
            big(3),
            big(412),
            varchar("Ito"),
            Value::Integer(500)
        ]]
    );
}

#[test]
fn aggregates_of_an_empty_table() {
    let mut db = database();
    db.execute("DELETE FROM emp").unwrap();
    let result = query(
        &mut db,
        "SELECT COUNT(*), SUM(salary), MAX(salary) FROM emp",
    );
    assert_eq!(result.rows, vec![vec![big(0), Value::Null, Value::Null]]);
    let result = query(&mut db, "SELECT dept, COUNT(*) FROM emp GROUP BY dept");
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn distinct_aggregates() {
    let result = query(
        &mut database(),
        "SELECT COUNT(DISTINCT dept), SUM(DISTINCT salary / 100) FROM emp",
    );
    assert_eq!(result.rows, vec![vec![big(2), big(12)]]);
}

#[test]
fn expressions_over_groups_and_aggregates() {
    let result = query(
        &mut database(),
        "SELECT dept || '!', MAX(salary) - MIN(salary) AS spread FROM emp WHERE dept IS NOT NULL GROUP BY dept ORDER BY spread DESC",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![varchar("dev!"), Value::Integer(50)],
            vec![varchar("ops!"), Value::Integer(0)],
        ]
    );
}

#[test]
fn having_without_group_by_filters_the_single_group() {
    let result = query(
        &mut database(),
        "SELECT COUNT(*) FROM emp HAVING COUNT(*) > 10",
    );
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn ungrouped_column_is_a_grouping_error() {
    let err = database()
        .execute("SELECT name, COUNT(*) FROM emp")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::GroupingError);
    assert_eq!(err.sqlstate().code(), "42803");
    assert_eq!(
        err.to_string(),
        "column \"NAME\" must appear in the GROUP BY clause or be used in an aggregate function"
    );
}

#[test]
fn aggregates_are_not_allowed_in_where_or_nested() {
    let mut db = database();
    let err = db
        .execute("SELECT dept FROM emp WHERE COUNT(*) > 1 GROUP BY dept")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::GroupingError);
    assert_eq!(
        err.to_string(),
        "aggregate functions are not allowed in WHERE"
    );
    let err = db.execute("SELECT MAX(COUNT(*)) FROM emp").unwrap_err();
    assert_eq!(err.to_string(), "aggregate function calls cannot be nested");
    let err = db.execute("VALUES (COUNT(*))").unwrap_err();
    assert_eq!(
        err.to_string(),
        "aggregate functions are not allowed in VALUES"
    );
}

#[test]
fn sum_of_strings_is_an_undefined_function() {
    let err = database().execute("SELECT SUM(name) FROM emp").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedFunction);
    assert_eq!(err.sqlstate().code(), "42883");
    assert_eq!(
        err.to_string(),
        "function sum(character varying) does not exist"
    );
}

#[test]
fn having_must_be_boolean() {
    let err = database()
        .execute("SELECT COUNT(*) FROM emp HAVING COUNT(*)")
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "argument of HAVING must be type boolean, not type bigint"
    );
}
