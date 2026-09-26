use winnow::Parser;
use winnow::combinator::{Infix, Postfix, Prefix, alt, delimited, expression, preceded, separated};
use winnow::stream::TokenSlice;
use winnow::token::{any, literal};

use crate::ast::{ArithmeticOp, BinaryOp, ComparisonOp, Expr, UnaryOp, Values};
use crate::token::{Keyword, Token};

/// 構文解析のエラー．
#[derive(Debug, PartialEq)]
pub struct ParseError;

type Tokens<'t> = TokenSlice<'t, Token>;

/// 演算子の優先順位．値が大きいほど強く結び付く．
const OR: i64 = 1;
const AND: i64 = 2;
const NOT: i64 = 3;
const IS: i64 = 4;
const COMPARISON: i64 = 5;
const CONCAT: i64 = 7;
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
        .prefix(alt((
            literal(Token::Minus).value(Prefix(UNARY, negate)),
            literal(Token::Keyword(Keyword::Not)).value(Prefix(NOT, not)),
        )))
        .postfix(alt((
            (
                literal(Token::Keyword(Keyword::Is)),
                literal(Token::Keyword(Keyword::Null)),
            )
                .value(Postfix(IS, is_null)),
            (
                literal(Token::Keyword(Keyword::Is)),
                literal(Token::Keyword(Keyword::Not)),
                literal(Token::Keyword(Keyword::Null)),
            )
                .value(Postfix(IS, is_not_null)),
        )))
        .infix(alt((
            alt((
                literal(Token::Plus).value(Infix::Left(ADDITIVE, add)),
                literal(Token::Minus).value(Infix::Left(ADDITIVE, subtract)),
                literal(Token::Star).value(Infix::Left(MULTIPLICATIVE, multiply)),
                literal(Token::Slash).value(Infix::Left(MULTIPLICATIVE, divide)),
            )),
            alt((
                literal(Token::Eq).value(Infix::Neither(COMPARISON, equal)),
                literal(Token::NotEq).value(Infix::Neither(COMPARISON, not_equal)),
                literal(Token::Lt).value(Infix::Neither(COMPARISON, less)),
                literal(Token::LtEq).value(Infix::Neither(COMPARISON, less_or_equal)),
                literal(Token::Gt).value(Infix::Neither(COMPARISON, greater)),
                literal(Token::GtEq).value(Infix::Neither(COMPARISON, greater_or_equal)),
            )),
            literal(Token::Concat).value(Infix::Left(CONCAT, concat)),
            literal(Token::Keyword(Keyword::And)).value(Infix::Left(AND, and)),
            literal(Token::Keyword(Keyword::Or)).value(Infix::Left(OR, or)),
        )))
        .parse_next(input)
}

fn operand(input: &mut Tokens<'_>) -> winnow::Result<Expr> {
    alt((
        any.verify_map(constant),
        delimited(literal(Token::LParen), expr, literal(Token::RParen)),
    ))
    .parse_next(input)
}

fn constant(token: &Token) -> Option<Expr> {
    match token {
        Token::Integer(n) => Some(Expr::Integer(*n)),
        Token::String(s) => Some(Expr::String(s.clone())),
        Token::Keyword(Keyword::True) => Some(Expr::Boolean(true)),
        Token::Keyword(Keyword::False) => Some(Expr::Boolean(false)),
        Token::Keyword(Keyword::Unknown | Keyword::Null) => Some(Expr::Null),
        _ => None,
    }
}

fn negate(_: &mut Tokens<'_>, operand: Expr) -> winnow::Result<Expr> {
    Ok(unary(UnaryOp::Neg, operand))
}

fn not(_: &mut Tokens<'_>, operand: Expr) -> winnow::Result<Expr> {
    Ok(unary(UnaryOp::Not, operand))
}

fn is_null(_: &mut Tokens<'_>, operand: Expr) -> winnow::Result<Expr> {
    Ok(Expr::IsNull {
        operand: Box::new(operand),
        negated: false,
    })
}

fn is_not_null(_: &mut Tokens<'_>, operand: Expr) -> winnow::Result<Expr> {
    Ok(Expr::IsNull {
        operand: Box::new(operand),
        negated: true,
    })
}

fn add(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(arithmetic(ArithmeticOp::Add, left, right))
}

fn subtract(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(arithmetic(ArithmeticOp::Sub, left, right))
}

fn multiply(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(arithmetic(ArithmeticOp::Mul, left, right))
}

fn divide(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(arithmetic(ArithmeticOp::Div, left, right))
}

fn equal(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(comparison(ComparisonOp::Eq, left, right))
}

fn not_equal(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(comparison(ComparisonOp::NotEq, left, right))
}

fn less(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(comparison(ComparisonOp::Lt, left, right))
}

fn less_or_equal(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(comparison(ComparisonOp::LtEq, left, right))
}

fn greater(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(comparison(ComparisonOp::Gt, left, right))
}

fn greater_or_equal(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(comparison(ComparisonOp::GtEq, left, right))
}

fn concat(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(binary(BinaryOp::Concat, left, right))
}

fn and(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(binary(BinaryOp::And, left, right))
}

fn or(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(binary(BinaryOp::Or, left, right))
}

fn unary(op: UnaryOp, operand: Expr) -> Expr {
    Expr::Unary {
        op,
        operand: Box::new(operand),
    }
}

fn arithmetic(op: ArithmeticOp, left: Expr, right: Expr) -> Expr {
    binary(BinaryOp::Arithmetic(op), left, right)
}

fn comparison(op: ComparisonOp, left: Expr, right: Expr) -> Expr {
    binary(BinaryOp::Comparison(op), left, right)
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
                    arithmetic(ArithmeticOp::Add, int(1), int(2)),
                    arithmetic(ArithmeticOp::Sub, int(1), int(2)),
                    arithmetic(ArithmeticOp::Mul, int(1), int(2)),
                    arithmetic(ArithmeticOp::Div, int(1), int(2)),
                ]
            })
        );
    }

    #[test]
    fn binary_operators_are_left_associative() {
        assert_eq!(
            parse_sql("VALUES (1 - 2 - 3)"),
            Ok(Values {
                exprs: vec![arithmetic(
                    ArithmeticOp::Sub,
                    arithmetic(ArithmeticOp::Sub, int(1), int(2)),
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
                exprs: vec![arithmetic(
                    ArithmeticOp::Add,
                    int(1),
                    arithmetic(ArithmeticOp::Mul, int(2), int(3))
                )]
            })
        );
    }

    #[test]
    fn parentheses_group_an_expression() {
        assert_eq!(
            parse_sql("VALUES ((1 + 2) * 3)"),
            Ok(Values {
                exprs: vec![arithmetic(
                    ArithmeticOp::Mul,
                    arithmetic(ArithmeticOp::Add, int(1), int(2)),
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
                exprs: vec![arithmetic(
                    ArithmeticOp::Mul,
                    unary(UnaryOp::Neg, int(2)),
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

    #[test]
    fn boolean_and_null_literals() {
        assert_eq!(
            parse_sql("VALUES (TRUE, FALSE, NULL, UNKNOWN)"),
            Ok(Values {
                exprs: vec![
                    Expr::Boolean(true),
                    Expr::Boolean(false),
                    Expr::Null,
                    Expr::Null
                ]
            })
        );
    }

    #[test]
    fn each_comparison_operator() {
        assert_eq!(
            parse_sql("VALUES (1 = 2, 1 <> 2, 1 < 2, 1 <= 2, 1 > 2, 1 >= 2)"),
            Ok(Values {
                exprs: vec![
                    comparison(ComparisonOp::Eq, int(1), int(2)),
                    comparison(ComparisonOp::NotEq, int(1), int(2)),
                    comparison(ComparisonOp::Lt, int(1), int(2)),
                    comparison(ComparisonOp::LtEq, int(1), int(2)),
                    comparison(ComparisonOp::Gt, int(1), int(2)),
                    comparison(ComparisonOp::GtEq, int(1), int(2)),
                ]
            })
        );
    }

    #[test]
    fn arithmetic_binds_tighter_than_comparison() {
        assert_eq!(
            parse_sql("VALUES (1 + 2 < 4)"),
            Ok(Values {
                exprs: vec![comparison(
                    ComparisonOp::Lt,
                    arithmetic(ArithmeticOp::Add, int(1), int(2)),
                    int(4)
                )]
            })
        );
    }

    #[test]
    fn comparisons_cannot_be_chained() {
        assert_eq!(parse_sql("VALUES (1 < 2 < 3)"), Err(ParseError));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        assert_eq!(
            parse_sql("VALUES (TRUE OR FALSE AND FALSE)"),
            Ok(Values {
                exprs: vec![binary(
                    BinaryOp::Or,
                    Expr::Boolean(true),
                    binary(BinaryOp::And, Expr::Boolean(false), Expr::Boolean(false))
                )]
            })
        );
    }

    #[test]
    fn not_binds_looser_than_comparison() {
        assert_eq!(
            parse_sql("VALUES (NOT 1 = 2)"),
            Ok(Values {
                exprs: vec![unary(
                    UnaryOp::Not,
                    comparison(ComparisonOp::Eq, int(1), int(2))
                )]
            })
        );
    }

    #[test]
    fn not_binds_tighter_than_and() {
        assert_eq!(
            parse_sql("VALUES (NOT TRUE AND FALSE)"),
            Ok(Values {
                exprs: vec![binary(
                    BinaryOp::And,
                    unary(UnaryOp::Not, Expr::Boolean(true)),
                    Expr::Boolean(false)
                )]
            })
        );
    }

    #[test]
    fn is_null_and_is_not_null() {
        assert_eq!(
            parse_sql("VALUES (1 IS NULL, 1 IS NOT NULL)"),
            Ok(Values {
                exprs: vec![
                    Expr::IsNull {
                        operand: Box::new(int(1)),
                        negated: false
                    },
                    Expr::IsNull {
                        operand: Box::new(int(1)),
                        negated: true
                    },
                ]
            })
        );
    }

    #[test]
    fn is_null_applies_to_the_whole_arithmetic_expression() {
        assert_eq!(
            parse_sql("VALUES (1 + 2 IS NULL)"),
            Ok(Values {
                exprs: vec![Expr::IsNull {
                    operand: Box::new(arithmetic(ArithmeticOp::Add, int(1), int(2))),
                    negated: false
                }]
            })
        );
    }

    fn string(s: &str) -> Expr {
        Expr::String(s.to_string())
    }

    #[test]
    fn string_literal() {
        assert_eq!(
            parse_sql("VALUES ('abc')"),
            Ok(Values {
                exprs: vec![string("abc")]
            })
        );
    }

    #[test]
    fn concatenation_is_left_associative() {
        assert_eq!(
            parse_sql("VALUES ('a' || 'b' || 'c')"),
            Ok(Values {
                exprs: vec![binary(
                    BinaryOp::Concat,
                    binary(BinaryOp::Concat, string("a"), string("b")),
                    string("c")
                )]
            })
        );
    }

    #[test]
    fn addition_binds_tighter_than_concatenation() {
        assert_eq!(
            parse_sql("VALUES (1 + 2 || 'x')"),
            Ok(Values {
                exprs: vec![binary(
                    BinaryOp::Concat,
                    arithmetic(ArithmeticOp::Add, int(1), int(2)),
                    string("x")
                )]
            })
        );
    }

    #[test]
    fn concatenation_binds_tighter_than_comparison() {
        assert_eq!(
            parse_sql("VALUES ('a' || 'b' = 'ab')"),
            Ok(Values {
                exprs: vec![comparison(
                    ComparisonOp::Eq,
                    binary(BinaryOp::Concat, string("a"), string("b")),
                    string("ab")
                )]
            })
        );
    }
}
