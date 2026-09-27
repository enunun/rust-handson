use std::sync::Arc;

use ferrodb::{Database, QueryResult, Session, SqlState, StatementResult, Value};

fn query(sql: &str) -> QueryResult {
    match Session::new(Arc::new(Database::new()))
        .execute(sql)
        .unwrap()
    {
        StatementResult::Rows(result) => result,
        other => panic!("not a query: {other:?}"),
    }
}

#[test]
fn evaluates_each_expression_of_a_row() {
    let result = query("VALUES (1 + 2 * 3, -(4 - 6) / 2)");
    assert_eq!(result.columns, vec!["COLUMN1", "COLUMN2"]);
    assert_eq!(
        result.rows,
        vec![vec![Value::Integer(7), Value::Integer(1)]]
    );
}

#[test]
fn evaluates_several_rows() {
    let result = query("VALUES (1, 'a'), (2, 'b')");
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Integer(1), Value::Varchar("a".to_string())],
            vec![Value::Integer(2), Value::Varchar("b".to_string())],
        ]
    );
}

#[test]
fn rows_of_different_lengths_are_a_syntax_error() {
    let err = Session::new(Arc::new(Database::new()))
        .execute("VALUES (1, 2), (3)")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::SyntaxError);
    assert_eq!(err.to_string(), "VALUES lists must all be the same length");
}

#[test]
fn evaluates_three_valued_logic() {
    let result = query("VALUES (1 < 2 AND NULL, NULL IS NULL, 1 + NULL)");
    assert_eq!(
        result.rows,
        vec![vec![Value::Null, Value::Boolean(true), Value::Null]]
    );
}

#[test]
fn concatenates_and_compares_strings() {
    let result = query("VALUES ('it''s' || ' ok', 'a' < 'b')");
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Varchar("it's ok".to_string()),
            Value::Boolean(true)
        ]]
    );
}

#[test]
fn reports_a_syntax_error_with_its_position() {
    let err = Session::new(Arc::new(Database::new()))
        .execute("VALUES (1 +)")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::SyntaxError);
    assert_eq!(err.sqlstate().code(), "42601");
    assert_eq!(err.position(), Some(12));
    assert_eq!(err.to_string(), "syntax error at or near \")\"");
}

#[test]
fn reports_a_syntax_error_at_the_end_of_input() {
    let err = Session::new(Arc::new(Database::new()))
        .execute("VALUES (1 +")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::SyntaxError);
    assert_eq!(err.position(), None);
    assert_eq!(err.to_string(), "syntax error at end of input");
}

#[test]
fn reports_a_lexical_error_as_a_syntax_error() {
    let err = Session::new(Arc::new(Database::new()))
        .execute("VALUES ('日本語' ? 1)")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::SyntaxError);
    assert_eq!(err.position(), Some(15));
    assert_eq!(err.to_string(), "syntax error at or near \"?\"");
}

#[test]
fn reports_numeric_value_out_of_range() {
    let err = Session::new(Arc::new(Database::new()))
        .execute("VALUES (2147483647 + 1)")
        .unwrap_err();
    assert_eq!(err.sqlstate().code(), "22003");
    assert_eq!(err.to_string(), "integer out of range");
}

#[test]
fn reports_division_by_zero() {
    let err = Session::new(Arc::new(Database::new()))
        .execute("VALUES (1 / 0)")
        .unwrap_err();
    assert_eq!(err.sqlstate().code(), "22012");
    assert_eq!(err.to_string(), "division by zero");
}

#[test]
fn reports_a_datatype_mismatch() {
    let err = Session::new(Arc::new(Database::new()))
        .execute("VALUES (1 + TRUE)")
        .unwrap_err();
    assert_eq!(err.sqlstate().code(), "42804");
    assert_eq!(err.position(), None);
}
