use winnow::Parser;
use winnow::combinator::{Infix, Prefix, alt, delimited, expression, preceded, separated};
use winnow::stream::TokenSlice;
use winnow::token::{any, literal};

use crate::ast::{BinaryOp, Expr, UnaryOp, Values};
use crate::token::{Keyword, Token};

/// 構文解析のエラー．
#[derive(Debug, PartialEq)]
pub struct ParseError;

type Tokens<'t> = TokenSlice<'t, Token>;

/// 演算子の優先順位．値が大きいほど強く結び付く．
const ADDITIVE: i64 = 10;
const MULTIPLICATIVE: i64 = 20;
const UNARY: i64 = 30;

/// トークンの列を`VALUES`の構文木にする．
pub fn parse(tokens: &[Token]) -> Result<Values, ParseError> {
    match values.parse(TokenSlice::new(tokens)) {
        Ok(values) => Ok(values),
        Err(_) => Err(ParseError),
    }
}

fn values(input: &mut Tokens<'_>) -> winnow::Result<Values> {
    preceded(
        literal(Token::Keyword(Keyword::Values)),
        delimited(
            literal(Token::LParen),
            separated(1.., expr, literal(Token::Comma)),
            literal(Token::RParen),
        ),
    )
    .map(to_values)
    .parse_next(input)
}

fn to_values(exprs: Vec<Expr>) -> Values {
    Values { exprs }
}

fn expr(input: &mut Tokens<'_>) -> winnow::Result<Expr> {
    expression(operand)
        .prefix(literal(Token::Minus).value(Prefix(UNARY, negate)))
        .infix(alt((
            literal(Token::Plus).value(Infix::Left(ADDITIVE, add)),
            literal(Token::Minus).value(Infix::Left(ADDITIVE, subtract)),
            literal(Token::Star).value(Infix::Left(MULTIPLICATIVE, multiply)),
            literal(Token::Slash).value(Infix::Left(MULTIPLICATIVE, divide)),
        )))
        .parse_next(input)
}

fn operand(input: &mut Tokens<'_>) -> winnow::Result<Expr> {
    alt((
        any.verify_map(integer),
        delimited(literal(Token::LParen), expr, literal(Token::RParen)),
    ))
    .parse_next(input)
}

fn integer(token: &Token) -> Option<Expr> {
    match token {
        Token::Integer(n) => Some(Expr::Integer(*n)),
        _ => None,
    }
}

fn negate(_: &mut Tokens<'_>, operand: Expr) -> winnow::Result<Expr> {
    Ok(Expr::Unary {
        op: UnaryOp::Neg,
        operand: Box::new(operand),
    })
}

fn add(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(binary(BinaryOp::Add, left, right))
}

fn subtract(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(binary(BinaryOp::Sub, left, right))
}

fn multiply(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(binary(BinaryOp::Mul, left, right))
}

fn divide(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(binary(BinaryOp::Div, left, right))
}

fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_sql(sql: &str) -> Result<Values, ParseError> {
        parse(&tokenize(sql).unwrap())
    }

    fn int(n: i64) -> Expr {
        Expr::Integer(n)
    }

    #[test]
    fn values_with_one_integer() {
        assert_eq!(
            parse_sql("VALUES (1)"),
            Ok(Values {
                exprs: vec![int(1)]
            })
        );
    }

    #[test]
    fn values_with_several_expressions() {
        assert_eq!(
            parse_sql("VALUES (1, 2, 3)"),
            Ok(Values {
                exprs: vec![int(1), int(2), int(3)]
            })
        );
    }

    #[test]
    fn each_binary_operator() {
        assert_eq!(
            parse_sql("VALUES (1 + 2, 1 - 2, 1 * 2, 1 / 2)"),
            Ok(Values {
                exprs: vec![
                    binary(BinaryOp::Add, int(1), int(2)),
                    binary(BinaryOp::Sub, int(1), int(2)),
                    binary(BinaryOp::Mul, int(1), int(2)),
                    binary(BinaryOp::Div, int(1), int(2)),
                ]
            })
        );
    }

    #[test]
    fn binary_operators_are_left_associative() {
        assert_eq!(
            parse_sql("VALUES (1 - 2 - 3)"),
            Ok(Values {
                exprs: vec![binary(
                    BinaryOp::Sub,
                    binary(BinaryOp::Sub, int(1), int(2)),
                    int(3)
                )]
            })
        );
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(
            parse_sql("VALUES (1 + 2 * 3)"),
            Ok(Values {
                exprs: vec![binary(
                    BinaryOp::Add,
                    int(1),
                    binary(BinaryOp::Mul, int(2), int(3))
                )]
            })
        );
    }

    #[test]
    fn parentheses_group_an_expression() {
        assert_eq!(
            parse_sql("VALUES ((1 + 2) * 3)"),
            Ok(Values {
                exprs: vec![binary(
                    BinaryOp::Mul,
                    binary(BinaryOp::Add, int(1), int(2)),
                    int(3)
                )]
            })
        );
    }

    #[test]
    fn unary_minus_binds_tighter_than_multiplication() {
        assert_eq!(
            parse_sql("VALUES (-2 * 3)"),
            Ok(Values {
                exprs: vec![binary(
                    BinaryOp::Mul,
                    Expr::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(int(2))
                    },
                    int(3)
                )]
            })
        );
    }

    #[test]
    fn missing_operand_is_an_error() {
        assert_eq!(parse_sql("VALUES (1 +)"), Err(ParseError));
    }

    #[test]
    fn empty_parentheses_are_an_error() {
        assert_eq!(parse_sql("VALUES ()"), Err(ParseError));
    }

    #[test]
    fn values_without_parentheses_is_an_error() {
        assert_eq!(parse_sql("VALUES 1"), Err(ParseError));
    }

    #[test]
    fn tokens_after_the_statement_are_an_error() {
        assert_eq!(parse_sql("VALUES (1) 2"), Err(ParseError));
    }
}
