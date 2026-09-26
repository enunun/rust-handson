use winnow::combinator::{
    Infix, Postfix, Prefix, alt, cut_err, delimited, expression, opt, preceded, separated,
    terminated,
};
use winnow::stream::TokenSlice;
use winnow::token::{any, literal};
use winnow::{ModalResult, Parser};

use crate::ast::{
    ArithmeticOp, BinaryOp, ColumnDef, ComparisonOp, CreateTable, Expr, Insert, Select, Statement,
    UnaryOp, Values,
};
use crate::token::{Keyword, Spanned, Token};
use crate::value::DataType;

/// 構文解析のエラー．読めなかったトークンがあればそのトークンと位置を，
/// 文が途中で終わっていれば`UnexpectedEnd`を返す．
/// `VALUES`の行によって値の数が違えば`ValuesLengthMismatch`を返す．
#[derive(Debug, PartialEq)]
pub enum ParseError {
    UnexpectedToken { token: Token, position: usize },
    UnexpectedEnd,
    ValuesLengthMismatch,
}

type Tokens<'t> = TokenSlice<'t, Spanned<Token>>;

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

/// トークンの列を文の構文木にする．
pub fn parse(tokens: &[Spanned<Token>]) -> Result<Statement, ParseError> {
    match statement.parse(TokenSlice::new(tokens)) {
        Ok(statement) => {
            match &statement {
                Statement::Values(values) => check_row_lengths(values)?,
                Statement::Insert(insert) => check_row_lengths(&insert.values)?,
                Statement::CreateTable(_) | Statement::Select(_) => {}
            }
            Ok(statement)
        }
        Err(error) => match tokens.get(error.offset()) {
            Some(token) => Err(ParseError::UnexpectedToken {
                token: token.value.clone(),
                position: token.position,
            }),
            None => Err(ParseError::UnexpectedEnd),
        },
    }
}

/// `VALUES`のすべての行が，同じ数の値を持つことを確かめる．
fn check_row_lengths(values: &Values) -> Result<(), ParseError> {
    let width = values.rows[0].len();
    for row in &values.rows {
        if row.len() != width {
            return Err(ParseError::ValuesLengthMismatch);
        }
    }
    Ok(())
}

fn statement(input: &mut Tokens<'_>) -> ModalResult<Statement> {
    alt((
        values.map(Statement::Values),
        create_table.map(Statement::CreateTable),
        insert.map(Statement::Insert),
        select.map(Statement::Select),
    ))
    .parse_next(input)
}

fn keyword(keyword: Keyword) -> Token {
    Token::Keyword(keyword)
}

fn values(input: &mut Tokens<'_>) -> ModalResult<Values> {
    preceded(
        literal(keyword(Keyword::Values)),
        cut_err(separated(1.., row, literal(Token::Comma))),
    )
    .map(to_values)
    .parse_next(input)
}

fn to_values(rows: Vec<Vec<Expr>>) -> Values {
    Values { rows }
}

fn row(input: &mut Tokens<'_>) -> ModalResult<Vec<Expr>> {
    delimited(
        literal(Token::LParen),
        separated(1.., cut_err(expr), literal(Token::Comma)),
        literal(Token::RParen),
    )
    .parse_next(input)
}

fn create_table(input: &mut Tokens<'_>) -> ModalResult<CreateTable> {
    preceded(
        (
            literal(keyword(Keyword::Create)),
            literal(keyword(Keyword::Table)),
        ),
        cut_err((
            identifier,
            delimited(
                literal(Token::LParen),
                separated(1.., column_def, literal(Token::Comma)),
                literal(Token::RParen),
            ),
        )),
    )
    .map(to_create_table)
    .parse_next(input)
}

fn to_create_table((name, columns): (String, Vec<ColumnDef>)) -> CreateTable {
    CreateTable { name, columns }
}

fn column_def(input: &mut Tokens<'_>) -> ModalResult<ColumnDef> {
    (identifier, data_type).map(to_column_def).parse_next(input)
}

fn to_column_def((name, data_type): (String, DataType)) -> ColumnDef {
    ColumnDef { name, data_type }
}

fn data_type(input: &mut Tokens<'_>) -> ModalResult<DataType> {
    alt((
        alt((
            literal(keyword(Keyword::Integer)),
            literal(keyword(Keyword::Int)),
        ))
        .value(DataType::Integer),
        literal(keyword(Keyword::Bigint)).value(DataType::BigInt),
        literal(keyword(Keyword::Boolean)).value(DataType::Boolean),
        preceded(
            literal(keyword(Keyword::Varchar)),
            delimited(
                literal(Token::LParen),
                any.verify_map(varchar_length),
                literal(Token::RParen),
            ),
        )
        .map(DataType::Varchar),
    ))
    .parse_next(input)
}

/// `VARCHAR(n)`の長さを読む．長さは1以上でなければならない．
fn varchar_length(token: &Spanned<Token>) -> Option<usize> {
    match token.value {
        Token::Integer(n) if n >= 1 => usize::try_from(n).ok(),
        _ => None,
    }
}

fn insert(input: &mut Tokens<'_>) -> ModalResult<Insert> {
    preceded(
        (
            literal(keyword(Keyword::Insert)),
            literal(keyword(Keyword::Into)),
        ),
        cut_err((
            identifier,
            opt(delimited(
                literal(Token::LParen),
                separated(1.., identifier, literal(Token::Comma)),
                literal(Token::RParen),
            )),
            values,
        )),
    )
    .map(to_insert)
    .parse_next(input)
}

fn to_insert((table, columns, values): (String, Option<Vec<String>>, Values)) -> Insert {
    Insert {
        table,
        columns,
        values,
    }
}

fn select(input: &mut Tokens<'_>) -> ModalResult<Select> {
    preceded(
        literal(keyword(Keyword::Select)),
        cut_err(preceded(
            (literal(Token::Star), literal(keyword(Keyword::From))),
            identifier,
        )),
    )
    .map(to_select)
    .parse_next(input)
}

fn to_select(table: String) -> Select {
    Select { table }
}

fn identifier(input: &mut Tokens<'_>) -> ModalResult<String> {
    any.verify_map(identifier_name).parse_next(input)
}

fn identifier_name(token: &Spanned<Token>) -> Option<String> {
    match &token.value {
        Token::Identifier(name) => Some(name.clone()),
        _ => None,
    }
}

fn expr(input: &mut Tokens<'_>) -> ModalResult<Expr> {
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

fn operand(input: &mut Tokens<'_>) -> ModalResult<Expr> {
    alt((
        any.verify_map(constant),
        preceded(
            literal(Token::LParen),
            cut_err(terminated(expr, literal(Token::RParen))),
        ),
    ))
    .parse_next(input)
}

fn constant(token: &Spanned<Token>) -> Option<Expr> {
    match &token.value {
        Token::Integer(n) => Some(Expr::Integer(*n)),
        Token::String(s) => Some(Expr::String(s.clone())),
        Token::Keyword(Keyword::True) => Some(Expr::Boolean(true)),
        Token::Keyword(Keyword::False) => Some(Expr::Boolean(false)),
        Token::Keyword(Keyword::Unknown | Keyword::Null) => Some(Expr::Null),
        _ => None,
    }
}

fn negate(_: &mut Tokens<'_>, operand: Expr) -> ModalResult<Expr> {
    Ok(unary(UnaryOp::Neg, operand))
}

fn not(_: &mut Tokens<'_>, operand: Expr) -> ModalResult<Expr> {
    Ok(unary(UnaryOp::Not, operand))
}

fn is_null(_: &mut Tokens<'_>, operand: Expr) -> ModalResult<Expr> {
    Ok(Expr::IsNull {
        operand: Box::new(operand),
        negated: false,
    })
}

fn is_not_null(_: &mut Tokens<'_>, operand: Expr) -> ModalResult<Expr> {
    Ok(Expr::IsNull {
        operand: Box::new(operand),
        negated: true,
    })
}

fn add(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(arithmetic(ArithmeticOp::Add, left, right))
}

fn subtract(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(arithmetic(ArithmeticOp::Sub, left, right))
}

fn multiply(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(arithmetic(ArithmeticOp::Mul, left, right))
}

fn divide(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(arithmetic(ArithmeticOp::Div, left, right))
}

fn equal(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(comparison(ComparisonOp::Eq, left, right))
}

fn not_equal(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(comparison(ComparisonOp::NotEq, left, right))
}

fn less(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(comparison(ComparisonOp::Lt, left, right))
}

fn less_or_equal(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(comparison(ComparisonOp::LtEq, left, right))
}

fn greater(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(comparison(ComparisonOp::Gt, left, right))
}

fn greater_or_equal(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(comparison(ComparisonOp::GtEq, left, right))
}

fn concat(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(binary(BinaryOp::Concat, left, right))
}

fn and(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
    Ok(binary(BinaryOp::And, left, right))
}

fn or(_: &mut Tokens<'_>, left: Expr, right: Expr) -> ModalResult<Expr> {
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

    /// `VALUES`の文を構文解析し，1行目の式の並びを返す．
    fn parse_sql(sql: &str) -> Result<Vec<Expr>, ParseError> {
        match parse(&tokenize(sql).unwrap())? {
            Statement::Values(mut values) => Ok(values.rows.remove(0)),
            other => panic!("not a VALUES statement: {other:?}"),
        }
    }

    fn int(n: i64) -> Expr {
        Expr::Integer(n)
    }

    #[test]
    fn values_with_one_integer() {
        assert_eq!(parse_sql("VALUES (1)"), Ok(vec![int(1)]));
    }

    #[test]
    fn values_with_several_expressions() {
        assert_eq!(
            parse_sql("VALUES (1, 2, 3)"),
            Ok(vec![int(1), int(2), int(3)])
        );
    }

    #[test]
    fn each_binary_operator() {
        assert_eq!(
            parse_sql("VALUES (1 + 2, 1 - 2, 1 * 2, 1 / 2)"),
            Ok(vec![
                arithmetic(ArithmeticOp::Add, int(1), int(2)),
                arithmetic(ArithmeticOp::Sub, int(1), int(2)),
                arithmetic(ArithmeticOp::Mul, int(1), int(2)),
                arithmetic(ArithmeticOp::Div, int(1), int(2)),
            ])
        );
    }

    #[test]
    fn binary_operators_are_left_associative() {
        assert_eq!(
            parse_sql("VALUES (1 - 2 - 3)"),
            Ok(vec![arithmetic(
                ArithmeticOp::Sub,
                arithmetic(ArithmeticOp::Sub, int(1), int(2)),
                int(3)
            )])
        );
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(
            parse_sql("VALUES (1 + 2 * 3)"),
            Ok(vec![arithmetic(
                ArithmeticOp::Add,
                int(1),
                arithmetic(ArithmeticOp::Mul, int(2), int(3))
            )])
        );
    }

    #[test]
    fn parentheses_group_an_expression() {
        assert_eq!(
            parse_sql("VALUES ((1 + 2) * 3)"),
            Ok(vec![arithmetic(
                ArithmeticOp::Mul,
                arithmetic(ArithmeticOp::Add, int(1), int(2)),
                int(3)
            )])
        );
    }

    #[test]
    fn unary_minus_binds_tighter_than_multiplication() {
        assert_eq!(
            parse_sql("VALUES (-2 * 3)"),
            Ok(vec![arithmetic(
                ArithmeticOp::Mul,
                unary(UnaryOp::Neg, int(2)),
                int(3)
            )])
        );
    }

    #[test]
    fn missing_operand_is_an_error() {
        assert_eq!(
            parse_sql("VALUES (1 +)"),
            Err(ParseError::UnexpectedToken {
                token: Token::RParen,
                position: 12
            })
        );
    }

    #[test]
    fn empty_parentheses_are_an_error() {
        assert_eq!(
            parse_sql("VALUES ()"),
            Err(ParseError::UnexpectedToken {
                token: Token::RParen,
                position: 9
            })
        );
    }

    #[test]
    fn values_without_parentheses_is_an_error() {
        assert_eq!(
            parse_sql("VALUES 1"),
            Err(ParseError::UnexpectedToken {
                token: Token::Integer(1),
                position: 8
            })
        );
    }

    #[test]
    fn tokens_after_the_statement_are_an_error() {
        assert_eq!(
            parse_sql("VALUES (1) 2"),
            Err(ParseError::UnexpectedToken {
                token: Token::Integer(2),
                position: 12
            })
        );
    }

    #[test]
    fn boolean_and_null_literals() {
        assert_eq!(
            parse_sql("VALUES (TRUE, FALSE, NULL, UNKNOWN)"),
            Ok(vec![
                Expr::Boolean(true),
                Expr::Boolean(false),
                Expr::Null,
                Expr::Null
            ])
        );
    }

    #[test]
    fn each_comparison_operator() {
        assert_eq!(
            parse_sql("VALUES (1 = 2, 1 <> 2, 1 < 2, 1 <= 2, 1 > 2, 1 >= 2)"),
            Ok(vec![
                comparison(ComparisonOp::Eq, int(1), int(2)),
                comparison(ComparisonOp::NotEq, int(1), int(2)),
                comparison(ComparisonOp::Lt, int(1), int(2)),
                comparison(ComparisonOp::LtEq, int(1), int(2)),
                comparison(ComparisonOp::Gt, int(1), int(2)),
                comparison(ComparisonOp::GtEq, int(1), int(2)),
            ])
        );
    }

    #[test]
    fn arithmetic_binds_tighter_than_comparison() {
        assert_eq!(
            parse_sql("VALUES (1 + 2 < 4)"),
            Ok(vec![comparison(
                ComparisonOp::Lt,
                arithmetic(ArithmeticOp::Add, int(1), int(2)),
                int(4)
            )])
        );
    }

    #[test]
    fn comparisons_cannot_be_chained() {
        assert_eq!(
            parse_sql("VALUES (1 < 2 < 3)"),
            Err(ParseError::UnexpectedToken {
                token: Token::Lt,
                position: 15
            })
        );
    }

    #[test]
    fn and_binds_tighter_than_or() {
        assert_eq!(
            parse_sql("VALUES (TRUE OR FALSE AND FALSE)"),
            Ok(vec![binary(
                BinaryOp::Or,
                Expr::Boolean(true),
                binary(BinaryOp::And, Expr::Boolean(false), Expr::Boolean(false))
            )])
        );
    }

    #[test]
    fn not_binds_looser_than_comparison() {
        assert_eq!(
            parse_sql("VALUES (NOT 1 = 2)"),
            Ok(vec![unary(
                UnaryOp::Not,
                comparison(ComparisonOp::Eq, int(1), int(2))
            )])
        );
    }

    #[test]
    fn not_binds_tighter_than_and() {
        assert_eq!(
            parse_sql("VALUES (NOT TRUE AND FALSE)"),
            Ok(vec![binary(
                BinaryOp::And,
                unary(UnaryOp::Not, Expr::Boolean(true)),
                Expr::Boolean(false)
            )])
        );
    }

    #[test]
    fn is_null_and_is_not_null() {
        assert_eq!(
            parse_sql("VALUES (1 IS NULL, 1 IS NOT NULL)"),
            Ok(vec![
                Expr::IsNull {
                    operand: Box::new(int(1)),
                    negated: false
                },
                Expr::IsNull {
                    operand: Box::new(int(1)),
                    negated: true
                },
            ])
        );
    }

    #[test]
    fn is_null_applies_to_the_whole_arithmetic_expression() {
        assert_eq!(
            parse_sql("VALUES (1 + 2 IS NULL)"),
            Ok(vec![Expr::IsNull {
                operand: Box::new(arithmetic(ArithmeticOp::Add, int(1), int(2))),
                negated: false
            }])
        );
    }

    fn string(s: &str) -> Expr {
        Expr::String(s.to_string())
    }

    #[test]
    fn string_literal() {
        assert_eq!(parse_sql("VALUES ('abc')"), Ok(vec![string("abc")]));
    }

    #[test]
    fn concatenation_is_left_associative() {
        assert_eq!(
            parse_sql("VALUES ('a' || 'b' || 'c')"),
            Ok(vec![binary(
                BinaryOp::Concat,
                binary(BinaryOp::Concat, string("a"), string("b")),
                string("c")
            )])
        );
    }

    #[test]
    fn addition_binds_tighter_than_concatenation() {
        assert_eq!(
            parse_sql("VALUES (1 + 2 || 'x')"),
            Ok(vec![binary(
                BinaryOp::Concat,
                arithmetic(ArithmeticOp::Add, int(1), int(2)),
                string("x")
            )])
        );
    }

    #[test]
    fn concatenation_binds_tighter_than_comparison() {
        assert_eq!(
            parse_sql("VALUES ('a' || 'b' = 'ab')"),
            Ok(vec![comparison(
                ComparisonOp::Eq,
                binary(BinaryOp::Concat, string("a"), string("b")),
                string("ab")
            )])
        );
    }

    #[test]
    fn error_inside_parentheses_reports_the_inner_token() {
        assert_eq!(
            parse_sql("VALUES ((1 +))"),
            Err(ParseError::UnexpectedToken {
                token: Token::RParen,
                position: 13
            })
        );
    }

    #[test]
    fn missing_expression_after_a_comma_reports_the_next_token() {
        assert_eq!(
            parse_sql("VALUES (1, )"),
            Err(ParseError::UnexpectedToken {
                token: Token::RParen,
                position: 12
            })
        );
    }

    #[test]
    fn statement_ending_too_early_is_an_unexpected_end() {
        assert_eq!(parse_sql("VALUES (1 +"), Err(ParseError::UnexpectedEnd));
    }

    fn statement(sql: &str) -> Result<Statement, ParseError> {
        parse(&tokenize(sql).unwrap())
    }

    #[test]
    fn values_with_several_rows() {
        assert_eq!(
            statement("VALUES (1, 2), (3, 4)"),
            Ok(Statement::Values(Values {
                rows: vec![vec![int(1), int(2)], vec![int(3), int(4)]]
            }))
        );
    }

    #[test]
    fn values_rows_of_different_lengths_are_an_error() {
        assert_eq!(
            statement("VALUES (1, 2), (3)"),
            Err(ParseError::ValuesLengthMismatch)
        );
    }

    #[test]
    fn create_table_with_each_data_type() {
        assert_eq!(
            statement("CREATE TABLE t (a INTEGER, b INT, c BIGINT, d BOOLEAN, e VARCHAR(10))"),
            Ok(Statement::CreateTable(CreateTable {
                name: "T".to_string(),
                columns: vec![
                    ColumnDef {
                        name: "A".to_string(),
                        data_type: DataType::Integer
                    },
                    ColumnDef {
                        name: "B".to_string(),
                        data_type: DataType::Integer
                    },
                    ColumnDef {
                        name: "C".to_string(),
                        data_type: DataType::BigInt
                    },
                    ColumnDef {
                        name: "D".to_string(),
                        data_type: DataType::Boolean
                    },
                    ColumnDef {
                        name: "E".to_string(),
                        data_type: DataType::Varchar(10)
                    },
                ]
            }))
        );
    }

    #[test]
    fn varchar_length_must_be_positive() {
        assert_eq!(
            statement("CREATE TABLE t (a VARCHAR(0))"),
            Err(ParseError::UnexpectedToken {
                token: Token::Integer(0),
                position: 27
            })
        );
    }

    #[test]
    fn create_table_needs_a_column() {
        assert_eq!(
            statement("CREATE TABLE t ()"),
            Err(ParseError::UnexpectedToken {
                token: Token::RParen,
                position: 17
            })
        );
    }

    #[test]
    fn insert_into_all_columns() {
        assert_eq!(
            statement("INSERT INTO t VALUES (1, 'a')"),
            Ok(Statement::Insert(Insert {
                table: "T".to_string(),
                columns: None,
                values: Values {
                    rows: vec![vec![int(1), string("a")]]
                }
            }))
        );
    }

    #[test]
    fn insert_into_named_columns() {
        assert_eq!(
            statement("INSERT INTO t (b, a) VALUES (1, 2)"),
            Ok(Statement::Insert(Insert {
                table: "T".to_string(),
                columns: Some(vec!["B".to_string(), "A".to_string()]),
                values: Values {
                    rows: vec![vec![int(1), int(2)]]
                }
            }))
        );
    }

    #[test]
    fn select_all_columns() {
        assert_eq!(
            statement("SELECT * FROM t"),
            Ok(Statement::Select(Select {
                table: "T".to_string()
            }))
        );
    }

    #[test]
    fn select_needs_from() {
        assert_eq!(statement("SELECT *"), Err(ParseError::UnexpectedEnd));
    }
}
