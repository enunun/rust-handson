use std::cmp::Ordering;

use crate::plan::binder::BoundExpr;
use crate::sql::ast::{ArithmeticOp, BinaryOp, ComparisonOp, UnaryOp};
use crate::value::Value;

/// 式の評価のエラー．
#[derive(Debug, PartialEq)]
pub enum EvalError {
    NumericOutOfRange,
    DivisionByZero,
    DatatypeMismatch,
    ArgumentNotBoolean {
        clause: &'static str,
        found: &'static str,
    },
}

/// 名前を解決した式を，行`row`について評価して値を返す．
pub fn eval(expr: &BoundExpr, row: &[Value]) -> Result<Value, EvalError> {
    match expr {
        BoundExpr::Constant(value) => Ok(value.clone()),
        BoundExpr::Column(index) => Ok(row[*index].clone()),
        BoundExpr::Unary { op, operand } => {
            let value = eval(operand, row)?;
            eval_unary(op, value)
        }
        BoundExpr::Binary { op, left, right } => {
            let left = eval(left, row)?;
            let right = eval(right, row)?;
            eval_binary(op, left, right)
        }
        BoundExpr::IsNull { operand, negated } => {
            let is_null = eval(operand, row)?.is_null();
            Ok(Value::Boolean(is_null != *negated))
        }
    }
}

/// `WHERE`などの条件を評価し，真なら`true`を返す．偽と`NULL`は`false`である．
/// 条件が真偽値でなければ，`clause`を含むエラーを返す．
pub fn eval_condition(
    expr: &BoundExpr,
    row: &[Value],
    clause: &'static str,
) -> Result<bool, EvalError> {
    match eval(expr, row)? {
        Value::Boolean(b) => Ok(b),
        Value::Null => Ok(false),
        other => Err(EvalError::ArgumentNotBoolean {
            clause,
            found: other.type_name(),
        }),
    }
}

/// 行が`WHERE`の条件を満たすかを返す．条件がなければ，すべての行が満たす．
pub fn matches_filter(filter: &Option<BoundExpr>, row: &[Value]) -> Result<bool, EvalError> {
    match filter {
        Some(filter) => eval_condition(filter, row, "WHERE"),
        None => Ok(true),
    }
}

fn eval_unary(op: &UnaryOp, value: Value) -> Result<Value, EvalError> {
    match op {
        UnaryOp::Neg => match value {
            Value::Integer(n) => match n.checked_neg() {
                Some(n) => Ok(Value::Integer(n)),
                None => Err(EvalError::NumericOutOfRange),
            },
            Value::BigInt(n) => match n.checked_neg() {
                Some(n) => Ok(Value::BigInt(n)),
                None => Err(EvalError::NumericOutOfRange),
            },
            Value::Null => Ok(Value::Null),
            Value::Boolean(_) | Value::Varchar(_) => Err(EvalError::DatatypeMismatch),
        },
        UnaryOp::Not => {
            let truth = truth(&value)?;
            Ok(Value::from_truth(not(truth)))
        }
    }
}

fn eval_binary(op: &BinaryOp, left: Value, right: Value) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Arithmetic(op) => arithmetic(op, left, right),
        BinaryOp::Comparison(op) => comparison(op, left, right),
        BinaryOp::Concat => concat(left, right),
        BinaryOp::And => Ok(Value::from_truth(and(truth(&left)?, truth(&right)?))),
        BinaryOp::Or => Ok(Value::from_truth(or(truth(&left)?, truth(&right)?))),
    }
}

fn arithmetic(op: &ArithmeticOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let (a, b) = match (integer_of(&left), integer_of(&right)) {
        (Some(a), Some(b)) => (a, b),
        _ => return null_or_mismatch(&left, &right),
    };
    let result = match op {
        ArithmeticOp::Add => a.checked_add(b),
        ArithmeticOp::Sub => a.checked_sub(b),
        ArithmeticOp::Mul => a.checked_mul(b),
        ArithmeticOp::Div => {
            if b == 0 {
                return Err(EvalError::DivisionByZero);
            }
            a.checked_div(b)
        }
    };
    let n = match result {
        Some(n) => n,
        None => return Err(EvalError::NumericOutOfRange),
    };
    // 両辺がINTEGERなら結果もINTEGER，片方でもBIGINTなら結果はBIGINTにする．
    if matches!(left, Value::BigInt(_)) || matches!(right, Value::BigInt(_)) {
        Ok(Value::BigInt(n))
    } else {
        match i32::try_from(n) {
            Ok(n) => Ok(Value::Integer(n)),
            Err(_) => Err(EvalError::NumericOutOfRange),
        }
    }
}

/// 整数の値を`i64`として読む．整数でなければ`None`を返す．
fn integer_of(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(n) => Some(i64::from(*n)),
        Value::BigInt(n) => Some(*n),
        Value::Boolean(_) | Value::Varchar(_) | Value::Null => None,
    }
}

/// 両辺が整数でない算術の結果．`NULL`と整数か`NULL`の組なら`NULL`，それ以外は型の不一致である．
fn null_or_mismatch(left: &Value, right: &Value) -> Result<Value, EvalError> {
    match (left, right) {
        (Value::Null, Value::Integer(_) | Value::BigInt(_) | Value::Null)
        | (Value::Integer(_) | Value::BigInt(_), Value::Null) => Ok(Value::Null),
        _ => Err(EvalError::DatatypeMismatch),
    }
}

fn comparison(op: &ComparisonOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let ordering = match (&left, &right) {
        (Value::Null, _) | (_, Value::Null) => return Ok(Value::Null),
        (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
        (Value::Varchar(a), Value::Varchar(b)) => a.cmp(b),
        _ => match (integer_of(&left), integer_of(&right)) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => return Err(EvalError::DatatypeMismatch),
        },
    };
    let result = match op {
        ComparisonOp::Eq => ordering == Ordering::Equal,
        ComparisonOp::NotEq => ordering != Ordering::Equal,
        ComparisonOp::Lt => ordering == Ordering::Less,
        ComparisonOp::LtEq => ordering != Ordering::Greater,
        ComparisonOp::Gt => ordering == Ordering::Greater,
        ComparisonOp::GtEq => ordering != Ordering::Less,
    };
    Ok(Value::Boolean(result))
}

fn concat(left: Value, right: Value) -> Result<Value, EvalError> {
    match (left, right) {
        (Value::Varchar(mut a), Value::Varchar(b)) => {
            a.push_str(&b);
            Ok(Value::Varchar(a))
        }
        (Value::Null, Value::Varchar(_) | Value::Null) | (Value::Varchar(_), Value::Null) => {
            Ok(Value::Null)
        }
        _ => Err(EvalError::DatatypeMismatch),
    }
}

/// 値を3値の真理値として読む．`None`は不明(`UNKNOWN`)を表す．
fn truth(value: &Value) -> Result<Option<bool>, EvalError> {
    match value {
        Value::Boolean(b) => Ok(Some(*b)),
        Value::Null => Ok(None),
        Value::Integer(_) | Value::BigInt(_) | Value::Varchar(_) => {
            Err(EvalError::DatatypeMismatch)
        }
    }
}

fn not(a: Option<bool>) -> Option<bool> {
    match a {
        Some(true) => Some(false),
        Some(false) => Some(true),
        None => None,
    }
}

fn and(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

fn or(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::binder::bind;
    use crate::sql::ast::Statement;
    use crate::sql::lexer::tokenize;
    use crate::sql::parser::parse;

    fn eval_sql(expr: &str) -> Result<Value, EvalError> {
        match parse(&tokenize(&format!("VALUES ({expr})")).unwrap()).unwrap() {
            Statement::Values(values) => eval(&bind(&values.rows[0][0], &[]).unwrap(), &[]),
            other => panic!("not a VALUES statement: {other:?}"),
        }
    }

    #[test]
    fn integer_literal() {
        assert_eq!(eval_sql("7"), Ok(Value::Integer(7)));
    }

    #[test]
    fn four_arithmetic_operations() {
        assert_eq!(eval_sql("6 + 3"), Ok(Value::Integer(9)));
        assert_eq!(eval_sql("6 - 3"), Ok(Value::Integer(3)));
        assert_eq!(eval_sql("6 * 3"), Ok(Value::Integer(18)));
        assert_eq!(eval_sql("6 / 3"), Ok(Value::Integer(2)));
    }

    #[test]
    fn unary_minus() {
        assert_eq!(eval_sql("-(4 - 6)"), Ok(Value::Integer(2)));
    }

    #[test]
    fn division_truncates_toward_zero() {
        assert_eq!(eval_sql("7 / 2"), Ok(Value::Integer(3)));
        assert_eq!(eval_sql("-7 / 2"), Ok(Value::Integer(-3)));
    }

    #[test]
    fn division_by_zero_is_an_error() {
        assert_eq!(eval_sql("1 / 0"), Err(EvalError::DivisionByZero));
    }

    #[test]
    fn literal_beyond_integer_is_a_bigint() {
        assert_eq!(eval_sql("2147483647"), Ok(Value::Integer(i32::MAX)));
        assert_eq!(eval_sql("2147483648"), Ok(Value::BigInt(2147483648)));
    }

    #[test]
    fn arithmetic_with_a_bigint_is_bigint() {
        assert_eq!(
            eval_sql("2147483647 + 2147483648"),
            Ok(Value::BigInt(4294967295))
        );
        assert_eq!(eval_sql("2147483648 - 1"), Ok(Value::BigInt(2147483647)));
    }

    #[test]
    fn overflowing_bigint_is_out_of_range() {
        assert_eq!(
            eval_sql("9223372036854775807 + 1"),
            Err(EvalError::NumericOutOfRange)
        );
        assert_eq!(
            eval_sql("-(-9223372036854775807 - 1)"),
            Err(EvalError::NumericOutOfRange)
        );
    }

    #[test]
    fn integer_and_bigint_compare_by_value() {
        assert_eq!(eval_sql("1 < 2147483648"), Ok(Value::Boolean(true)));
        assert_eq!(
            eval_sql("2147483648 = 2147483648"),
            Ok(Value::Boolean(true))
        );
    }

    #[test]
    fn bigint_with_null_is_null() {
        assert_eq!(eval_sql("2147483648 + NULL"), Ok(Value::Null));
        assert_eq!(eval_sql("-NULL"), Ok(Value::Null));
    }

    #[test]
    fn overflowing_addition_is_out_of_range() {
        assert_eq!(
            eval_sql("2147483647 + 1"),
            Err(EvalError::NumericOutOfRange)
        );
    }

    #[test]
    fn overflowing_subtraction_is_out_of_range() {
        assert_eq!(
            eval_sql("-2147483647 - 2"),
            Err(EvalError::NumericOutOfRange)
        );
    }

    #[test]
    fn overflowing_multiplication_is_out_of_range() {
        assert_eq!(eval_sql("65536 * 32768"), Err(EvalError::NumericOutOfRange));
    }

    #[test]
    fn overflowing_negation_is_out_of_range() {
        assert_eq!(
            eval_sql("-(-2147483647 - 1)"),
            Err(EvalError::NumericOutOfRange)
        );
    }

    #[test]
    fn overflowing_division_is_out_of_range() {
        assert_eq!(
            eval_sql("(-2147483647 - 1) / -1"),
            Err(EvalError::NumericOutOfRange)
        );
    }

    #[test]
    fn boolean_literals() {
        assert_eq!(eval_sql("TRUE"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("FALSE"), Ok(Value::Boolean(false)));
    }

    #[test]
    fn null_and_unknown_are_null() {
        assert_eq!(eval_sql("NULL"), Ok(Value::Null));
        assert_eq!(eval_sql("UNKNOWN"), Ok(Value::Null));
    }

    #[test]
    fn comparison_of_integers() {
        assert_eq!(eval_sql("1 = 1"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("1 <> 1"), Ok(Value::Boolean(false)));
        assert_eq!(eval_sql("1 < 2"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("2 <= 1"), Ok(Value::Boolean(false)));
        assert_eq!(eval_sql("2 > 1"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("1 >= 1"), Ok(Value::Boolean(true)));
    }

    #[test]
    fn false_is_less_than_true() {
        assert_eq!(eval_sql("FALSE < TRUE"), Ok(Value::Boolean(true)));
    }

    #[test]
    fn comparison_with_null_is_unknown() {
        assert_eq!(eval_sql("1 = NULL"), Ok(Value::Null));
        assert_eq!(eval_sql("NULL = NULL"), Ok(Value::Null));
    }

    #[test]
    fn and_follows_three_valued_logic() {
        assert_eq!(eval_sql("TRUE AND TRUE"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("TRUE AND FALSE"), Ok(Value::Boolean(false)));
        assert_eq!(eval_sql("TRUE AND NULL"), Ok(Value::Null));
        assert_eq!(eval_sql("FALSE AND NULL"), Ok(Value::Boolean(false)));
        assert_eq!(eval_sql("NULL AND NULL"), Ok(Value::Null));
    }

    #[test]
    fn or_follows_three_valued_logic() {
        assert_eq!(eval_sql("FALSE OR FALSE"), Ok(Value::Boolean(false)));
        assert_eq!(eval_sql("FALSE OR TRUE"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("FALSE OR NULL"), Ok(Value::Null));
        assert_eq!(eval_sql("TRUE OR NULL"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("NULL OR NULL"), Ok(Value::Null));
    }

    #[test]
    fn not_follows_three_valued_logic() {
        assert_eq!(eval_sql("NOT TRUE"), Ok(Value::Boolean(false)));
        assert_eq!(eval_sql("NOT FALSE"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("NOT NULL"), Ok(Value::Null));
    }

    #[test]
    fn is_null_and_is_not_null() {
        assert_eq!(eval_sql("NULL IS NULL"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("1 IS NULL"), Ok(Value::Boolean(false)));
        assert_eq!(eval_sql("NULL IS NOT NULL"), Ok(Value::Boolean(false)));
        assert_eq!(eval_sql("1 IS NOT NULL"), Ok(Value::Boolean(true)));
    }

    #[test]
    fn arithmetic_with_null_is_null() {
        assert_eq!(eval_sql("1 + NULL"), Ok(Value::Null));
        assert_eq!(eval_sql("-NULL"), Ok(Value::Null));
        assert_eq!(eval_sql("NULL / 0"), Ok(Value::Null));
    }

    #[test]
    fn arithmetic_on_booleans_is_a_type_mismatch() {
        assert_eq!(eval_sql("1 + TRUE"), Err(EvalError::DatatypeMismatch));
        assert_eq!(eval_sql("-TRUE"), Err(EvalError::DatatypeMismatch));
        assert_eq!(eval_sql("TRUE + NULL"), Err(EvalError::DatatypeMismatch));
    }

    #[test]
    fn comparing_different_types_is_a_type_mismatch() {
        assert_eq!(eval_sql("1 = TRUE"), Err(EvalError::DatatypeMismatch));
    }

    #[test]
    fn logical_operators_on_integers_are_a_type_mismatch() {
        assert_eq!(eval_sql("1 AND TRUE"), Err(EvalError::DatatypeMismatch));
        assert_eq!(eval_sql("NOT 1"), Err(EvalError::DatatypeMismatch));
    }

    fn varchar(s: &str) -> Value {
        Value::Varchar(s.to_string())
    }

    #[test]
    fn string_literal() {
        assert_eq!(eval_sql("'abc'"), Ok(varchar("abc")));
    }

    #[test]
    fn concatenation() {
        assert_eq!(eval_sql("'it''s' || ' ok'"), Ok(varchar("it's ok")));
    }

    #[test]
    fn concatenation_with_null_is_null() {
        assert_eq!(eval_sql("'a' || NULL"), Ok(Value::Null));
        assert_eq!(eval_sql("NULL || 'a'"), Ok(Value::Null));
    }

    #[test]
    fn concatenating_a_non_string_is_a_type_mismatch() {
        assert_eq!(eval_sql("'a' || 1"), Err(EvalError::DatatypeMismatch));
    }

    #[test]
    fn strings_compare_by_code_point() {
        assert_eq!(eval_sql("'abc' = 'abc'"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("'a' < 'b'"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("'B' < 'a'"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("'' < 'a'"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("'ab' < 'b'"), Ok(Value::Boolean(true)));
        assert_eq!(eval_sql("'z' < 'あ'"), Ok(Value::Boolean(true)));
    }

    #[test]
    fn comparing_a_string_with_an_integer_is_a_type_mismatch() {
        assert_eq!(eval_sql("'1' = 1"), Err(EvalError::DatatypeMismatch));
    }

    #[test]
    fn arithmetic_and_logic_on_strings_are_type_mismatches() {
        assert_eq!(eval_sql("'a' + 1"), Err(EvalError::DatatypeMismatch));
        assert_eq!(eval_sql("-'a'"), Err(EvalError::DatatypeMismatch));
        assert_eq!(eval_sql("NOT 'a'"), Err(EvalError::DatatypeMismatch));
    }

    #[test]
    fn column_takes_its_value_from_the_row() {
        let row = vec![Value::Integer(3), Value::Varchar("x".to_string())];
        assert_eq!(
            eval(&BoundExpr::Column(1), &row),
            Ok(Value::Varchar("x".to_string()))
        );
    }

    #[test]
    fn condition_is_true_only_for_true() {
        let row = vec![Value::Boolean(true), Value::Boolean(false), Value::Null];
        assert_eq!(
            eval_condition(&BoundExpr::Column(0), &row, "WHERE"),
            Ok(true)
        );
        assert_eq!(
            eval_condition(&BoundExpr::Column(1), &row, "WHERE"),
            Ok(false)
        );
        assert_eq!(
            eval_condition(&BoundExpr::Column(2), &row, "WHERE"),
            Ok(false)
        );
    }

    #[test]
    fn condition_must_be_boolean() {
        assert_eq!(
            eval_condition(&BoundExpr::Constant(Value::Integer(1)), &[], "WHERE"),
            Err(EvalError::ArgumentNotBoolean {
                clause: "WHERE",
                found: "integer"
            })
        );
    }
}
