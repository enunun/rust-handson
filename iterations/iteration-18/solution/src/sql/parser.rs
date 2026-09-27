use winnow::combinator::{
    Infix, Postfix, Prefix, alt, cut_err, delimited, expression, opt, preceded, repeat, separated,
    terminated,
};
use winnow::stream::TokenSlice;
use winnow::token::{any, literal};
use winnow::{ModalResult, Parser};

use crate::sql::ast::{
    AggregateFunc, ArithmeticOp, Assignment, BinaryOp, ColumnConstraint, ColumnDef, ComparisonOp,
    CreateIndex, CreateTable, Delete, DropIndex, DropTable, Expr, Insert, Join, JoinKind, Limit,
    OrderBy, Select, SelectItem, Statement, TableRef, UnaryOp, Update, Values,
};
use crate::sql::token::{Keyword, Spanned, Token};
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
                Statement::CreateTable(_)
                | Statement::Select(_)
                | Statement::Update(_)
                | Statement::Delete(_)
                | Statement::DropTable(_)
                | Statement::CreateIndex(_)
                | Statement::DropIndex(_)
                | Statement::Explain(_)
                | Statement::StartTransaction
                | Statement::Commit
                | Statement::Rollback => {}
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
        create,
        insert.map(Statement::Insert),
        select.map(Statement::Select),
        update.map(Statement::Update),
        delete.map(Statement::Delete),
        drop,
        preceded(literal(keyword(Keyword::Explain)), cut_err(select)).map(Statement::Explain),
        transaction,
    ))
    .parse_next(input)
}

/// `CREATE TABLE`か`CREATE INDEX`．
fn create(input: &mut Tokens<'_>) -> ModalResult<Statement> {
    alt((
        create_table.map(Statement::CreateTable),
        create_index.map(Statement::CreateIndex),
    ))
    .parse_next(input)
}

/// `START TRANSACTION`，`COMMIT`，`ROLLBACK`．
fn transaction(input: &mut Tokens<'_>) -> ModalResult<Statement> {
    alt((
        (
            literal(keyword(Keyword::Start)),
            cut_err(literal(keyword(Keyword::Transaction))),
        )
            .map(|_| Statement::StartTransaction),
        literal(keyword(Keyword::Commit)).map(|_| Statement::Commit),
        literal(keyword(Keyword::Rollback)).map(|_| Statement::Rollback),
    ))
    .parse_next(input)
}

/// `DROP TABLE`か`DROP INDEX`．
fn drop(input: &mut Tokens<'_>) -> ModalResult<Statement> {
    alt((
        drop_table.map(Statement::DropTable),
        drop_index.map(Statement::DropIndex),
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
    (identifier, data_type, repeat(0.., column_constraint))
        .map(to_column_def)
        .parse_next(input)
}

fn column_constraint(input: &mut Tokens<'_>) -> ModalResult<ColumnConstraint> {
    alt((
        (
            literal(keyword(Keyword::Not)),
            literal(keyword(Keyword::Null)),
        )
            .value(ColumnConstraint::NotNull),
        (
            literal(keyword(Keyword::Primary)),
            literal(keyword(Keyword::Key)),
        )
            .value(ColumnConstraint::PrimaryKey),
        literal(keyword(Keyword::Unique)).value(ColumnConstraint::Unique),
    ))
    .parse_next(input)
}

fn to_column_def(
    (name, data_type, constraints): (String, DataType, Vec<ColumnConstraint>),
) -> ColumnDef {
    ColumnDef {
        name,
        data_type,
        constraints,
    }
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
        cut_err((
            opt(literal(keyword(Keyword::Distinct))).map(|distinct| distinct.is_some()),
            separated(1.., select_item, literal(Token::Comma)),
            preceded(literal(keyword(Keyword::From)), from_clause),
            opt(preceded(literal(keyword(Keyword::Where)), expr)),
            group_by,
            opt(preceded(literal(keyword(Keyword::Having)), expr)),
            order_by,
            limit,
        )),
    )
    .map(
        |(distinct, items, from, filter, group_by, having, order_by, limit)| Select {
            distinct,
            items,
            from,
            filter,
            group_by,
            having,
            order_by,
            limit,
        },
    )
    .parse_next(input)
}

/// `FROM`の表の並びを読む．`,`で区切った表は，左から順に`CROSS JOIN`でつなぐ．
fn from_clause(input: &mut Tokens<'_>) -> ModalResult<TableRef> {
    separated(1.., joined_table, literal(Token::Comma))
        .map(|tables: Vec<TableRef>| {
            tables
                .into_iter()
                .reduce(|left, right| join(left, right, JoinKind::Cross, None))
                .expect("FROM has at least one table")
        })
        .parse_next(input)
}

/// 表と，それに続く`JOIN`を読む．`JOIN`は左から順に結合する．
fn joined_table(input: &mut Tokens<'_>) -> ModalResult<TableRef> {
    (table_primary, repeat(0.., join_clause))
        .map(
            |(first, joins): (TableRef, Vec<(JoinKind, TableRef, Option<Expr>)>)| {
                joins
                    .into_iter()
                    .fold(first, |left, (kind, right, condition)| {
                        join(left, right, kind, condition)
                    })
            },
        )
        .parse_next(input)
}

/// 表の名前と，省略できる別名を読む．
fn table_primary(input: &mut Tokens<'_>) -> ModalResult<TableRef> {
    (
        identifier,
        opt(preceded(opt(literal(keyword(Keyword::As))), identifier)),
    )
        .map(|(name, alias)| TableRef::Table { name, alias })
        .parse_next(input)
}

/// `CROSS JOIN 表`，`[INNER] JOIN 表 ON 条件`，`LEFT [OUTER] JOIN 表 ON 条件`を読む．
fn join_clause(input: &mut Tokens<'_>) -> ModalResult<(JoinKind, TableRef, Option<Expr>)> {
    alt((
        preceded(
            (
                literal(keyword(Keyword::Cross)),
                literal(keyword(Keyword::Join)),
            ),
            cut_err(table_primary),
        )
        .map(|table| (JoinKind::Cross, table, None)),
        (
            join_kind,
            cut_err((table_primary, preceded(literal(keyword(Keyword::On)), expr))),
        )
            .map(|(kind, (table, condition))| (kind, table, Some(condition))),
    ))
    .parse_next(input)
}

fn join_kind(input: &mut Tokens<'_>) -> ModalResult<JoinKind> {
    alt((
        (
            opt(literal(keyword(Keyword::Inner))),
            literal(keyword(Keyword::Join)),
        )
            .value(JoinKind::Inner),
        (
            literal(keyword(Keyword::Left)),
            opt(literal(keyword(Keyword::Outer))),
            literal(keyword(Keyword::Join)),
        )
            .value(JoinKind::Left),
    ))
    .parse_next(input)
}

fn join(left: TableRef, right: TableRef, kind: JoinKind, condition: Option<Expr>) -> TableRef {
    TableRef::Join(Box::new(Join {
        left,
        right,
        kind,
        condition,
    }))
}

/// `GROUP BY`の式の並びを読む．`GROUP BY`がなければ空の並びを返す．
fn group_by(input: &mut Tokens<'_>) -> ModalResult<Vec<Expr>> {
    opt(preceded(
        (
            literal(keyword(Keyword::Group)),
            literal(keyword(Keyword::By)),
        ),
        cut_err(separated(1.., expr, literal(Token::Comma))),
    ))
    .map(|exprs| exprs.unwrap_or_default())
    .parse_next(input)
}

/// 集約関数の呼び出しを読む．`COUNT(*)`と，`関数([DISTINCT] 式)`である．
fn aggregate_call(input: &mut Tokens<'_>) -> ModalResult<Expr> {
    alt((
        (
            literal(keyword(Keyword::Count)),
            literal(Token::LParen),
            literal(Token::Star),
            cut_err(literal(Token::RParen)),
        )
            .map(|_| Expr::Aggregate {
                func: AggregateFunc::Count,
                arg: None,
                distinct: false,
            }),
        (
            aggregate_func,
            preceded(
                literal(Token::LParen),
                cut_err((
                    opt(literal(keyword(Keyword::Distinct))).map(|distinct| distinct.is_some()),
                    terminated(expr, literal(Token::RParen)),
                )),
            ),
        )
            .map(|(func, (distinct, arg))| Expr::Aggregate {
                func,
                arg: Some(Box::new(arg)),
                distinct,
            }),
    ))
    .parse_next(input)
}

fn aggregate_func(input: &mut Tokens<'_>) -> ModalResult<AggregateFunc> {
    alt((
        literal(keyword(Keyword::Count)).value(AggregateFunc::Count),
        literal(keyword(Keyword::Sum)).value(AggregateFunc::Sum),
        literal(keyword(Keyword::Avg)).value(AggregateFunc::Avg),
        literal(keyword(Keyword::Min)).value(AggregateFunc::Min),
        literal(keyword(Keyword::Max)).value(AggregateFunc::Max),
    ))
    .parse_next(input)
}

/// 列の名前か，表の名前で修飾した列の名前(`表.列`)を読む．
fn column_reference(input: &mut Tokens<'_>) -> ModalResult<Expr> {
    (
        identifier,
        opt(preceded(literal(Token::Dot), cut_err(identifier))),
    )
        .map(|(first, second)| match second {
            Some(column) => Expr::QualifiedColumn {
                table: first,
                column,
            },
            None => Expr::Column(first),
        })
        .parse_next(input)
}

/// `ORDER BY`のキーの並びを読む．`ORDER BY`がなければ空の並びを返す．
fn order_by(input: &mut Tokens<'_>) -> ModalResult<Vec<OrderBy>> {
    opt(preceded(
        (
            literal(keyword(Keyword::Order)),
            literal(keyword(Keyword::By)),
        ),
        cut_err(separated(1.., order_by_item, literal(Token::Comma))),
    ))
    .map(|keys| keys.unwrap_or_default())
    .parse_next(input)
}

fn order_by_item(input: &mut Tokens<'_>) -> ModalResult<OrderBy> {
    (
        expr,
        opt(alt((
            literal(keyword(Keyword::Asc)).value(false),
            literal(keyword(Keyword::Desc)).value(true),
        ))),
        opt(preceded(
            literal(keyword(Keyword::Nulls)),
            alt((
                literal(keyword(Keyword::First)).value(true),
                literal(keyword(Keyword::Last)).value(false),
            )),
        )),
    )
        .map(|(expr, descending, nulls_first)| OrderBy {
            expr,
            descending: descending.unwrap_or(false),
            nulls_first,
        })
        .parse_next(input)
}

/// `OFFSET n ROWS`と`FETCH FIRST n ROWS ONLY`を読む．どちらも省略できる．
fn limit(input: &mut Tokens<'_>) -> ModalResult<Limit> {
    (
        opt(preceded(
            literal(keyword(Keyword::Offset)),
            cut_err(terminated(row_count, literal(keyword(Keyword::Rows)))),
        )),
        opt(preceded(
            literal(keyword(Keyword::Fetch)),
            cut_err(delimited(
                literal(keyword(Keyword::First)),
                row_count,
                (
                    literal(keyword(Keyword::Rows)),
                    literal(keyword(Keyword::Only)),
                ),
            )),
        )),
    )
        .map(|(offset, fetch)| Limit {
            offset: offset.unwrap_or(0),
            fetch,
        })
        .parse_next(input)
}

/// 行の数を読む．0以上の整数である．
fn row_count(input: &mut Tokens<'_>) -> ModalResult<usize> {
    any.verify_map(|token: &Spanned<Token>| match token.value {
        Token::Integer(n) => usize::try_from(n).ok(),
        _ => None,
    })
    .parse_next(input)
}

fn update(input: &mut Tokens<'_>) -> ModalResult<Update> {
    preceded(
        literal(keyword(Keyword::Update)),
        cut_err((
            identifier,
            preceded(
                literal(keyword(Keyword::Set)),
                separated(1.., assignment, literal(Token::Comma)),
            ),
            opt(preceded(literal(keyword(Keyword::Where)), expr)),
        )),
    )
    .map(|(table, assignments, filter)| Update {
        table,
        assignments,
        filter,
    })
    .parse_next(input)
}

fn assignment(input: &mut Tokens<'_>) -> ModalResult<Assignment> {
    (identifier, preceded(literal(Token::Eq), expr))
        .map(|(column, value)| Assignment { column, value })
        .parse_next(input)
}

fn delete(input: &mut Tokens<'_>) -> ModalResult<Delete> {
    preceded(
        (
            literal(keyword(Keyword::Delete)),
            literal(keyword(Keyword::From)),
        ),
        cut_err((
            identifier,
            opt(preceded(literal(keyword(Keyword::Where)), expr)),
        )),
    )
    .map(|(table, filter)| Delete { table, filter })
    .parse_next(input)
}

fn drop_table(input: &mut Tokens<'_>) -> ModalResult<DropTable> {
    preceded(
        (
            literal(keyword(Keyword::Drop)),
            literal(keyword(Keyword::Table)),
        ),
        cut_err(identifier),
    )
    .map(|name| DropTable { name })
    .parse_next(input)
}

fn create_index(input: &mut Tokens<'_>) -> ModalResult<CreateIndex> {
    preceded(
        (
            literal(keyword(Keyword::Create)),
            literal(keyword(Keyword::Index)),
        ),
        cut_err((
            identifier,
            preceded(literal(keyword(Keyword::On)), identifier),
            delimited(literal(Token::LParen), identifier, literal(Token::RParen)),
        )),
    )
    .map(|(name, table, column)| CreateIndex {
        name,
        table,
        column,
    })
    .parse_next(input)
}

fn drop_index(input: &mut Tokens<'_>) -> ModalResult<DropIndex> {
    preceded(
        (
            literal(keyword(Keyword::Drop)),
            literal(keyword(Keyword::Index)),
        ),
        cut_err(identifier),
    )
    .map(|name| DropIndex { name })
    .parse_next(input)
}

fn select_item(input: &mut Tokens<'_>) -> ModalResult<SelectItem> {
    alt((
        literal(Token::Star).map(|_| SelectItem::Wildcard),
        (
            expr,
            opt(preceded(opt(literal(keyword(Keyword::As))), identifier)),
        )
            .map(to_select_expr),
    ))
    .parse_next(input)
}

fn to_select_expr((expr, alias): (Expr, Option<String>)) -> SelectItem {
    SelectItem::Expr { expr, alias }
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
        aggregate_call,
        column_reference,
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
    use crate::sql::lexer::tokenize;

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
                        data_type: DataType::Integer,
                        constraints: vec![]
                    },
                    ColumnDef {
                        name: "B".to_string(),
                        data_type: DataType::Integer,
                        constraints: vec![]
                    },
                    ColumnDef {
                        name: "C".to_string(),
                        data_type: DataType::BigInt,
                        constraints: vec![]
                    },
                    ColumnDef {
                        name: "D".to_string(),
                        data_type: DataType::Boolean,
                        constraints: vec![]
                    },
                    ColumnDef {
                        name: "E".to_string(),
                        data_type: DataType::Varchar(10),
                        constraints: vec![]
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
                distinct: false,
                items: vec![SelectItem::Wildcard],
                from: table("T"),
                filter: None,
                group_by: vec![],
                having: None,
                order_by: vec![],
                limit: Limit::default()
            }))
        );
    }

    #[test]
    fn select_needs_from() {
        assert_eq!(statement("SELECT *"), Err(ParseError::UnexpectedEnd));
    }

    fn column(name: &str) -> Expr {
        Expr::Column(name.to_string())
    }

    #[test]
    fn column_reference_is_an_operand() {
        assert_eq!(
            parse_sql("VALUES (a + 1)"),
            Ok(vec![arithmetic(ArithmeticOp::Add, column("A"), int(1))])
        );
    }

    #[test]
    fn select_expressions_with_and_without_alias() {
        assert_eq!(
            statement("SELECT a, b + 1 AS c, d e FROM t"),
            Ok(Statement::Select(Select {
                distinct: false,
                items: vec![
                    SelectItem::Expr {
                        expr: column("A"),
                        alias: None
                    },
                    SelectItem::Expr {
                        expr: arithmetic(ArithmeticOp::Add, column("B"), int(1)),
                        alias: Some("C".to_string())
                    },
                    SelectItem::Expr {
                        expr: column("D"),
                        alias: Some("E".to_string())
                    },
                ],
                from: table("T"),
                filter: None,
                group_by: vec![],
                having: None,
                order_by: vec![],
                limit: Limit::default()
            }))
        );
    }

    #[test]
    fn select_with_where() {
        assert_eq!(
            statement("SELECT * FROM t WHERE a > 1"),
            Ok(Statement::Select(Select {
                distinct: false,
                items: vec![SelectItem::Wildcard],
                from: table("T"),
                filter: Some(comparison(ComparisonOp::Gt, column("A"), int(1))),
                group_by: vec![],
                having: None,
                order_by: vec![],
                limit: Limit::default()
            }))
        );
    }

    #[test]
    fn select_needs_an_item() {
        assert_eq!(
            statement("SELECT FROM t"),
            Err(ParseError::UnexpectedToken {
                token: Token::Keyword(Keyword::From),
                position: 8
            })
        );
    }

    #[test]
    fn column_constraints() {
        assert_eq!(
            statement("CREATE TABLE t (a INTEGER PRIMARY KEY, b INTEGER NOT NULL UNIQUE)"),
            Ok(Statement::CreateTable(CreateTable {
                name: "T".to_string(),
                columns: vec![
                    ColumnDef {
                        name: "A".to_string(),
                        data_type: DataType::Integer,
                        constraints: vec![ColumnConstraint::PrimaryKey]
                    },
                    ColumnDef {
                        name: "B".to_string(),
                        data_type: DataType::Integer,
                        constraints: vec![ColumnConstraint::NotNull, ColumnConstraint::Unique]
                    },
                ]
            }))
        );
    }

    #[test]
    fn update_with_assignments_and_where() {
        assert_eq!(
            statement("UPDATE t SET a = a + 1, b = 'x' WHERE a > 1"),
            Ok(Statement::Update(Update {
                table: "T".to_string(),
                assignments: vec![
                    Assignment {
                        column: "A".to_string(),
                        value: arithmetic(ArithmeticOp::Add, column("A"), int(1))
                    },
                    Assignment {
                        column: "B".to_string(),
                        value: string("x")
                    },
                ],
                filter: Some(comparison(ComparisonOp::Gt, column("A"), int(1)))
            }))
        );
    }

    #[test]
    fn update_needs_an_assignment() {
        assert_eq!(
            statement("UPDATE t SET WHERE a > 1"),
            Err(ParseError::UnexpectedToken {
                token: Token::Keyword(Keyword::Where),
                position: 14
            })
        );
    }

    #[test]
    fn delete_with_and_without_where() {
        assert_eq!(
            statement("DELETE FROM t WHERE a = 1"),
            Ok(Statement::Delete(Delete {
                table: "T".to_string(),
                filter: Some(comparison(ComparisonOp::Eq, column("A"), int(1)))
            }))
        );
        assert_eq!(
            statement("DELETE FROM t"),
            Ok(Statement::Delete(Delete {
                table: "T".to_string(),
                filter: None
            }))
        );
    }

    #[test]
    fn create_index_names_one_column() {
        assert_eq!(
            statement("CREATE INDEX emp_salary ON emp (salary)"),
            Ok(Statement::CreateIndex(CreateIndex {
                name: "EMP_SALARY".to_string(),
                table: "EMP".to_string(),
                column: "SALARY".to_string(),
            }))
        );
        assert!(statement("CREATE INDEX i ON t (a, b)").is_err());
    }

    #[test]
    fn transaction_statements() {
        assert_eq!(
            statement("START TRANSACTION"),
            Ok(Statement::StartTransaction)
        );
        assert_eq!(statement("commit"), Ok(Statement::Commit));
        assert_eq!(statement("ROLLBACK"), Ok(Statement::Rollback));
        assert!(statement("START").is_err());
    }

    #[test]
    fn drop_index() {
        assert_eq!(
            statement("DROP INDEX emp_salary"),
            Ok(Statement::DropIndex(DropIndex {
                name: "EMP_SALARY".to_string()
            }))
        );
    }

    #[test]
    fn drop_table() {
        assert_eq!(
            statement("DROP TABLE t"),
            Ok(Statement::DropTable(DropTable {
                name: "T".to_string()
            }))
        );
    }

    fn order_by(expr: Expr, descending: bool, nulls_first: Option<bool>) -> OrderBy {
        OrderBy {
            expr,
            descending,
            nulls_first,
        }
    }

    fn select_with(sql: &str) -> Select {
        match statement(sql) {
            Ok(Statement::Select(select)) => select,
            other => panic!("not a SELECT statement: {other:?}"),
        }
    }

    #[test]
    fn order_by_with_directions_and_nulls() {
        assert_eq!(
            select_with("SELECT * FROM t ORDER BY a DESC, b + 1 ASC NULLS FIRST, c NULLS LAST")
                .order_by,
            vec![
                order_by(column("A"), true, None),
                order_by(
                    arithmetic(ArithmeticOp::Add, column("B"), int(1)),
                    false,
                    Some(true)
                ),
                order_by(column("C"), false, Some(false)),
            ]
        );
    }

    #[test]
    fn offset_and_fetch_first() {
        assert_eq!(
            select_with("SELECT * FROM t OFFSET 1 ROWS FETCH FIRST 2 ROWS ONLY").limit,
            Limit {
                offset: 1,
                fetch: Some(2)
            }
        );
        assert_eq!(
            select_with("SELECT * FROM t FETCH FIRST 0 ROWS ONLY").limit,
            Limit {
                offset: 0,
                fetch: Some(0)
            }
        );
    }

    #[test]
    fn row_count_must_be_an_integer() {
        assert_eq!(
            statement("SELECT * FROM t OFFSET a ROWS"),
            Err(ParseError::UnexpectedToken {
                token: Token::Identifier("A".to_string()),
                position: 24
            })
        );
    }

    #[test]
    fn select_distinct() {
        assert!(select_with("SELECT DISTINCT a FROM t").distinct);
        assert!(!select_with("SELECT a FROM t").distinct);
    }

    #[test]
    fn explain_takes_a_select() {
        assert_eq!(
            statement("EXPLAIN SELECT * FROM t"),
            Ok(Statement::Explain(select_with("SELECT * FROM t")))
        );
        assert_eq!(
            statement("EXPLAIN VALUES (1)"),
            Err(ParseError::UnexpectedToken {
                token: Token::Keyword(Keyword::Values),
                position: 9
            })
        );
    }

    fn table(name: &str) -> TableRef {
        TableRef::Table {
            name: name.to_string(),
            alias: None,
        }
    }

    #[test]
    fn qualified_column_reference() {
        assert_eq!(
            parse_sql("VALUES (e.name)"),
            Ok(vec![Expr::QualifiedColumn {
                table: "E".to_string(),
                column: "NAME".to_string()
            }])
        );
    }

    #[test]
    fn tables_in_from_are_cross_joined_from_the_left() {
        assert_eq!(
            select_with("SELECT * FROM a, b AS x, c y").from,
            join(
                join(
                    table("A"),
                    TableRef::Table {
                        name: "B".to_string(),
                        alias: Some("X".to_string())
                    },
                    JoinKind::Cross,
                    None
                ),
                TableRef::Table {
                    name: "C".to_string(),
                    alias: Some("Y".to_string())
                },
                JoinKind::Cross,
                None
            )
        );
    }

    #[test]
    fn each_kind_of_join() {
        let on = || Some(comparison(ComparisonOp::Eq, column("A"), column("B")));
        assert_eq!(
            select_with("SELECT * FROM a CROSS JOIN b").from,
            join(table("A"), table("B"), JoinKind::Cross, None)
        );
        assert_eq!(
            select_with("SELECT * FROM a JOIN b ON a = b").from,
            join(table("A"), table("B"), JoinKind::Inner, on())
        );
        assert_eq!(
            select_with("SELECT * FROM a INNER JOIN b ON a = b").from,
            join(table("A"), table("B"), JoinKind::Inner, on())
        );
        assert_eq!(
            select_with("SELECT * FROM a LEFT JOIN b ON a = b").from,
            join(table("A"), table("B"), JoinKind::Left, on())
        );
        assert_eq!(
            select_with("SELECT * FROM a LEFT OUTER JOIN b ON a = b").from,
            join(table("A"), table("B"), JoinKind::Left, on())
        );
    }

    #[test]
    fn joins_are_left_associative() {
        assert_eq!(
            select_with("SELECT * FROM a JOIN b ON TRUE LEFT JOIN c ON FALSE").from,
            join(
                join(
                    table("A"),
                    table("B"),
                    JoinKind::Inner,
                    Some(Expr::Boolean(true))
                ),
                table("C"),
                JoinKind::Left,
                Some(Expr::Boolean(false))
            )
        );
    }

    #[test]
    fn inner_join_needs_on() {
        assert_eq!(
            statement("SELECT * FROM a JOIN b"),
            Err(ParseError::UnexpectedEnd)
        );
    }

    fn aggregate(func: AggregateFunc, arg: Option<Expr>, distinct: bool) -> Expr {
        Expr::Aggregate {
            func,
            arg: arg.map(Box::new),
            distinct,
        }
    }

    #[test]
    fn aggregate_calls() {
        assert_eq!(
            parse_sql("VALUES (COUNT(*), count(a), SUM(DISTINCT a + 1), AVG(a), MIN(a), MAX(a))"),
            Ok(vec![
                aggregate(AggregateFunc::Count, None, false),
                aggregate(AggregateFunc::Count, Some(column("A")), false),
                aggregate(
                    AggregateFunc::Sum,
                    Some(arithmetic(ArithmeticOp::Add, column("A"), int(1))),
                    true
                ),
                aggregate(AggregateFunc::Avg, Some(column("A")), false),
                aggregate(AggregateFunc::Min, Some(column("A")), false),
                aggregate(AggregateFunc::Max, Some(column("A")), false),
            ])
        );
    }

    #[test]
    fn aggregate_needs_an_argument() {
        assert_eq!(
            statement("VALUES (SUM())"),
            Err(ParseError::UnexpectedToken {
                token: Token::RParen,
                position: 13
            })
        );
    }

    #[test]
    fn group_by_and_having() {
        let select = select_with("SELECT a FROM t GROUP BY a, b + 1 HAVING COUNT(*) > 1");
        assert_eq!(
            select.group_by,
            vec![
                column("A"),
                arithmetic(ArithmeticOp::Add, column("B"), int(1))
            ]
        );
        assert_eq!(
            select.having,
            Some(comparison(
                ComparisonOp::Gt,
                aggregate(AggregateFunc::Count, None, false),
                int(1)
            ))
        );
    }
}
