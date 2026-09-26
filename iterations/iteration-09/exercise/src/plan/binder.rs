//! 構文木の式の名前を解決する．列の名前を行の中の番号にし，リテラルを値にする．

use crate::catalog::Column;
use crate::sql::ast::{BinaryOp, Expr, UnaryOp};
use crate::value::Value;

/// 名前を解決した式．列は行の中の番号で指す．
#[derive(Debug, Clone, PartialEq)]
pub enum BoundExpr {
    Constant(Value),
    Column(usize),
    Unary {
        op: UnaryOp,
        operand: Box<BoundExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<BoundExpr>,
        right: Box<BoundExpr>,
    },
    IsNull {
        operand: Box<BoundExpr>,
        negated: bool,
    },
}

/// 名前解決のエラー．
#[derive(Debug, PartialEq)]
pub enum BindError {
    UndefinedColumn { column: String },
}

/// 式の中の列の名前を，`columns`の中の番号に解決する．
pub fn bind(expr: &Expr, columns: &[Column]) -> Result<BoundExpr, BindError> {
    let bound = match expr {
        Expr::Integer(n) => BoundExpr::Constant(match i32::try_from(*n) {
            Ok(n) => Value::Integer(n),
            Err(_) => Value::BigInt(*n),
        }),
        Expr::Boolean(b) => BoundExpr::Constant(Value::Boolean(*b)),
        Expr::String(s) => BoundExpr::Constant(Value::Varchar(s.clone())),
        Expr::Null => BoundExpr::Constant(Value::Null),
        Expr::Column(name) => {
            let index = columns
                .iter()
                .position(|column| column.name == *name)
                .ok_or_else(|| BindError::UndefinedColumn {
                    column: name.clone(),
                })?;
            BoundExpr::Column(index)
        }
        Expr::Unary { op, operand } => BoundExpr::Unary {
            op: op.clone(),
            operand: Box::new(bind(operand, columns)?),
        },
        Expr::Binary { op, left, right } => BoundExpr::Binary {
            op: op.clone(),
            left: Box::new(bind(left, columns)?),
            right: Box::new(bind(right, columns)?),
        },
        Expr::IsNull { operand, negated } => BoundExpr::IsNull {
            operand: Box::new(bind(operand, columns)?),
            negated: *negated,
        },
    };
    Ok(bound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::ast::{ArithmeticOp, Statement};
    use crate::sql::lexer::tokenize;
    use crate::sql::parser::parse;
    use crate::value::DataType;

    fn expr(sql: &str) -> Expr {
        match parse(&tokenize(&format!("VALUES ({sql})")).unwrap()).unwrap() {
            Statement::Values(mut values) => values.rows.remove(0).remove(0),
            other => panic!("not a VALUES statement: {other:?}"),
        }
    }

    fn columns() -> Vec<Column> {
        vec![
            Column {
                name: "ID".to_string(),
                data_type: DataType::Integer,
                nullable: true,
            },
            Column {
                name: "NAME".to_string(),
                data_type: DataType::Varchar(10),
                nullable: true,
            },
        ]
    }

    #[test]
    fn literals_become_constant_values() {
        assert_eq!(
            bind(&expr("1"), &[]),
            Ok(BoundExpr::Constant(Value::Integer(1)))
        );
        assert_eq!(
            bind(&expr("2147483648"), &[]),
            Ok(BoundExpr::Constant(Value::BigInt(2147483648)))
        );
        assert_eq!(
            bind(&expr("'a'"), &[]),
            Ok(BoundExpr::Constant(Value::Varchar("a".to_string())))
        );
        assert_eq!(
            bind(&expr("NULL"), &[]),
            Ok(BoundExpr::Constant(Value::Null))
        );
    }

    #[test]
    fn column_name_becomes_its_index() {
        assert_eq!(bind(&expr("name"), &columns()), Ok(BoundExpr::Column(1)));
    }

    #[test]
    fn columns_inside_an_expression_are_resolved() {
        assert_eq!(
            bind(&expr("id + 1"), &columns()),
            Ok(BoundExpr::Binary {
                op: BinaryOp::Arithmetic(ArithmeticOp::Add),
                left: Box::new(BoundExpr::Column(0)),
                right: Box::new(BoundExpr::Constant(Value::Integer(1)))
            })
        );
    }

    #[test]
    fn unknown_column_is_undefined() {
        assert_eq!(
            bind(&expr("-age"), &columns()),
            Err(BindError::UndefinedColumn {
                column: "AGE".to_string()
            })
        );
    }
}
