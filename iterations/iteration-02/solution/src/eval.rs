use std::cmp::Ordering;

use crate::ast::{ArithmeticOp, BinaryOp, ComparisonOp, Expr, UnaryOp};
use crate::value::Value;

/// 式の評価のエラー．
#[derive(Debug, PartialEq)]
pub enum EvalError {
    NumericOutOfRange,
    DivisionByZero,
    DatatypeMismatch,
}

/// 式を評価して値を返す．
pub fn eval(expr: &Expr) -> Result<Value, EvalError> {
    match expr {
        Expr::Integer(n) => match i32::try_from(*n) {
            Ok(n) => Ok(Value::Integer(n)),
            Err(_) => Err(EvalError::NumericOutOfRange),
        },
        Expr::Boolean(b) => Ok(Value::Boolean(*b)),
        Expr::Null => Ok(Value::Null),
        Expr::Unary { op, operand } => {
            let value = eval(operand)?;
            eval_unary(op, value)
        }
        Expr::Binary { op, left, right } => {
            let left = eval(left)?;
            let right = eval(right)?;
            eval_binary(op, left, right)
        }
        Expr::IsNull { operand, negated } => {
            let is_null = eval(operand)?.is_null();
            Ok(Value::Boolean(is_null != *negated))
        }
    }
}

fn eval_unary(op: &UnaryOp, value: Value) -> Result<Value, EvalError> {
    match op {
        UnaryOp::Neg => match value {
            Value::Integer(n) => integer_or_out_of_range(n.checked_neg()),
            Value::Null => Ok(Value::Null),
            Value::Boolean(_) => Err(EvalError::DatatypeMismatch),
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
        BinaryOp::And => Ok(Value::from_truth(and(truth(&left)?, truth(&right)?))),
        BinaryOp::Or => Ok(Value::from_truth(or(truth(&left)?, truth(&right)?))),
    }
}

fn arithmetic(op: &ArithmeticOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let (a, b) = match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => (a, b),
        (Value::Null, Value::Integer(_) | Value::Null) | (Value::Integer(_), Value::Null) => {
            return Ok(Value::Null);
        }
        _ => return Err(EvalError::DatatypeMismatch),
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
    integer_or_out_of_range(result)
}

fn comparison(op: &ComparisonOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let ordering = match (left, right) {
        (Value::Null, _) | (_, Value::Null) => return Ok(Value::Null),
        (Value::Integer(a), Value::Integer(b)) => a.cmp(&b),
        (Value::Boolean(a), Value::Boolean(b)) => a.cmp(&b),
        _ => return Err(EvalError::DatatypeMismatch),
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

/// 値を3値の真理値として読む．`None`は不明(`UNKNOWN`)を表す．
fn truth(value: &Value) -> Result<Option<bool>, EvalError> {
    match value {
        Value::Boolean(b) => Ok(Some(*b)),
        Value::Null => Ok(None),
        Value::Integer(_) => Err(EvalError::DatatypeMismatch),
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

fn integer_or_out_of_range(result: Option<i32>) -> Result<Value, EvalError> {
    match result {
        Some(n) => Ok(Value::Integer(n)),
        None => Err(EvalError::NumericOutOfRange),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::parse;

    fn eval_sql(expr: &str) -> Result<Value, EvalError> {
        let values = parse(&tokenize(&format!("VALUES ({expr})")).unwrap()).unwrap();
        eval(&values.exprs[0])
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
    fn literal_beyond_integer_is_out_of_range() {
        assert_eq!(eval_sql("2147483647"), Ok(Value::Integer(i32::MAX)));
        assert_eq!(eval_sql("2147483648"), Err(EvalError::NumericOutOfRange));
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
}
