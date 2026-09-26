use ferrodb::{Error, EvalError, LexError, ParseError, Value, execute};

#[test]
fn evaluates_each_expression_of_a_row() {
    let result = execute("VALUES (1 + 2 * 3, -(4 - 6) / 2)").unwrap();
    assert_eq!(result.columns, vec!["COLUMN1", "COLUMN2"]);
    assert_eq!(
        result.rows,
        vec![vec![Value::Integer(7), Value::Integer(1)]]
    );
}

#[test]
fn reports_a_lexical_error() {
    assert_eq!(
        execute("VALUES (1 ? 2)"),
        Err(Error::Lex(LexError { position: 11 }))
    );
}

#[test]
fn reports_a_syntax_error() {
    assert_eq!(execute("VALUES (1 +)"), Err(Error::Parse(ParseError)));
}

#[test]
fn reports_division_by_zero() {
    assert_eq!(
        execute("VALUES (1 / 0)"),
        Err(Error::Eval(EvalError::DivisionByZero))
    );
}

#[test]
fn evaluates_three_valued_logic() {
    let result = execute("VALUES (1 < 2 AND NULL, NULL IS NULL, 1 + NULL)").unwrap();
    assert_eq!(
        result.rows,
        vec![vec![Value::Null, Value::Boolean(true), Value::Null]]
    );
}

#[test]
fn reports_a_type_mismatch() {
    assert_eq!(
        execute("VALUES (1 + TRUE)"),
        Err(Error::Eval(EvalError::DatatypeMismatch))
    );
}

#[test]
fn concatenates_and_compares_strings() {
    let result = execute("VALUES ('it''s' || ' ok', 'a' < 'b')").unwrap();
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Varchar("it's ok".to_string()),
            Value::Boolean(true)
        ]]
    );
}

#[test]
fn reports_a_lexical_error_position_in_characters() {
    assert_eq!(
        execute("VALUES ('日本語' ? 1)"),
        Err(Error::Lex(LexError { position: 15 }))
    );
}
