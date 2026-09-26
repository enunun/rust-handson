use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::value::Value;

/// 式の評価のエラー．
#[derive(Debug, PartialEq)]
pub enum EvalError {
    NumericOutOfRange,
    DivisionByZero,
}

/// 式を評価して値を返す．
pub fn eval(expr: &Expr) -> Result<Value, EvalError> {
    match expr {
        Expr::Integer(n) => match i32::try_from(*n) {
            Ok(n) => Ok(Value::Integer(n)),
            Err(_) => Err(EvalError::NumericOutOfRange),
        },
        Expr::Unary { op, operand } => {
            let value = eval(operand)?;
            eval_unary(op, value)
        }
        Expr::Binary { op, left, right } => {
            let left = eval(left)?;
            let right = eval(right)?;
            eval_binary(op, left, right)
        }
    }
}

fn eval_unary(op: &UnaryOp, value: Value) -> Result<Value, EvalError> {
    let Value::Integer(n) = value;
    let result = match op {
        UnaryOp::Neg => n.checked_neg(),
    };
    integer_or_out_of_range(result)
}

fn eval_binary(op: &BinaryOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let Value::Integer(a) = left;
    let Value::Integer(b) = right;
    let result = match op {
        BinaryOp::Add => a.checked_add(b),
        BinaryOp::Sub => a.checked_sub(b),
        BinaryOp::Mul => a.checked_mul(b),
        BinaryOp::Div => {
            if b == 0 {
                return Err(EvalError::DivisionByZero);
            }
            a.checked_div(b)
        }
    };
    integer_or_out_of_range(result)
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
}
