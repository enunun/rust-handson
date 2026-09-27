use std::fmt;

use crate::catalog::SchemaError;
use crate::exec::dml::ConstraintError;
use crate::exec::eval::EvalError;
use crate::plan::binder::BindError;
use crate::sql::lexer::LexError;
use crate::sql::parser::ParseError;
use crate::storage::heap::HeapError;
use crate::storage::page::{MAX_TUPLE_SIZE, PageError};
use crate::storage::tuple::TupleError;

/// SQLSTATE．エラーの種類を表す5文字のコードに対応する．
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlState {
    SyntaxError,
    NumericValueOutOfRange,
    DivisionByZero,
    DatatypeMismatch,
    StringDataRightTruncation,
    UndefinedTable,
    DuplicateTable,
    UndefinedColumn,
    DuplicateColumn,
    InvalidTableDefinition,
    NotNullViolation,
    UniqueViolation,
    InvalidColumnReference,
    AmbiguousColumn,
    DuplicateAlias,
    GroupingError,
    UndefinedFunction,
    ProgramLimitExceeded,
    DataCorrupted,
    InternalError,
    IoError,
}

impl SqlState {
    /// SQLSTATEの5文字のコードを返す．
    pub fn code(self) -> &'static str {
        match self {
            SqlState::SyntaxError => "42601",
            SqlState::NumericValueOutOfRange => "22003",
            SqlState::DivisionByZero => "22012",
            SqlState::DatatypeMismatch => "42804",
            SqlState::StringDataRightTruncation => "22001",
            SqlState::UndefinedTable => "42P01",
            SqlState::DuplicateTable => "42P07",
            SqlState::UndefinedColumn => "42703",
            SqlState::DuplicateColumn => "42701",
            SqlState::InvalidTableDefinition => "42P16",
            SqlState::NotNullViolation => "23502",
            SqlState::UniqueViolation => "23505",
            SqlState::InvalidColumnReference => "42P10",
            SqlState::AmbiguousColumn => "42702",
            SqlState::DuplicateAlias => "42712",
            SqlState::GroupingError => "42803",
            SqlState::UndefinedFunction => "42883",
            SqlState::ProgramLimitExceeded => "54000",
            SqlState::DataCorrupted => "XX001",
            SqlState::InternalError => "XX000",
            SqlState::IoError => "58030",
        }
    }
}

/// SQLの実行のエラー．どの段階のエラーも，SQLSTATE，メッセージ，文の中の位置で表す．
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    sqlstate: SqlState,
    message: String,
    position: Option<usize>,
}

impl Error {
    pub fn sqlstate(&self) -> SqlState {
        self.sqlstate
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// エラーの原因の文字の位置(先頭の文字を1と数える)．位置のないエラーは`None`を返す．
    pub fn position(&self) -> Option<usize> {
        self.position
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<LexError> for Error {
    fn from(error: LexError) -> Error {
        Error {
            sqlstate: SqlState::SyntaxError,
            message: format!("syntax error at or near \"{}\"", error.found),
            position: Some(error.position),
        }
    }
}

impl From<ParseError> for Error {
    fn from(error: ParseError) -> Error {
        match error {
            ParseError::UnexpectedToken { token, position } => Error {
                sqlstate: SqlState::SyntaxError,
                message: format!("syntax error at or near \"{token}\""),
                position: Some(position),
            },
            ParseError::UnexpectedEnd => Error {
                sqlstate: SqlState::SyntaxError,
                message: "syntax error at end of input".to_string(),
                position: None,
            },
            ParseError::ValuesLengthMismatch => Error {
                sqlstate: SqlState::SyntaxError,
                message: "VALUES lists must all be the same length".to_string(),
                position: None,
            },
        }
    }
}

impl From<EvalError> for Error {
    fn from(error: EvalError) -> Error {
        let (sqlstate, message) = match error {
            EvalError::NumericOutOfRange => (
                SqlState::NumericValueOutOfRange,
                "integer out of range".to_string(),
            ),
            EvalError::DivisionByZero => (SqlState::DivisionByZero, "division by zero".to_string()),
            EvalError::DatatypeMismatch => {
                (SqlState::DatatypeMismatch, "datatype mismatch".to_string())
            }
            EvalError::UndefinedFunction { name, argument } => (
                SqlState::UndefinedFunction,
                format!(
                    "function {}({argument}) does not exist",
                    name.to_lowercase()
                ),
            ),
            EvalError::ArgumentNotBoolean { clause, found } => (
                SqlState::DatatypeMismatch,
                format!("argument of {clause} must be type boolean, not type {found}"),
            ),
        };
        Error {
            sqlstate,
            message,
            position: None,
        }
    }
}

impl From<BindError> for Error {
    fn from(error: BindError) -> Error {
        match error {
            BindError::UndefinedColumn { column } => Error {
                sqlstate: SqlState::UndefinedColumn,
                message: format!("column \"{column}\" does not exist"),
                position: None,
            },
            BindError::UndefinedQualifiedColumn { table, column } => Error {
                sqlstate: SqlState::UndefinedColumn,
                message: format!("column {table}.{column} does not exist"),
                position: None,
            },
            BindError::AmbiguousColumn { column } => Error {
                sqlstate: SqlState::AmbiguousColumn,
                message: format!("column reference \"{column}\" is ambiguous"),
                position: None,
            },
            BindError::UndefinedTable { table } => Error {
                sqlstate: SqlState::UndefinedTable,
                message: format!("relation \"{table}\" does not exist"),
                position: None,
            },
            BindError::MissingFromEntry { table } => Error {
                sqlstate: SqlState::UndefinedTable,
                message: format!("missing FROM-clause entry for table \"{table}\""),
                position: None,
            },
            BindError::DuplicateTableName { table } => Error {
                sqlstate: SqlState::DuplicateAlias,
                message: format!("table name \"{table}\" specified more than once"),
                position: None,
            },
            BindError::NotGrouped { column } => Error {
                sqlstate: SqlState::GroupingError,
                message: format!(
                    "column \"{column}\" must appear in the GROUP BY clause or be used in an aggregate function"
                ),
                position: None,
            },
            BindError::NestedAggregate => Error {
                sqlstate: SqlState::GroupingError,
                message: "aggregate function calls cannot be nested".to_string(),
                position: None,
            },
            BindError::AggregateNotAllowed { clause } => Error {
                sqlstate: SqlState::GroupingError,
                message: format!("aggregate functions are not allowed in {clause}"),
                position: None,
            },
            BindError::MisplacedAggregate => Error {
                sqlstate: SqlState::GroupingError,
                message: "aggregate functions are not allowed here".to_string(),
                position: None,
            },
            BindError::OrderByNotInSelectList => Error {
                sqlstate: SqlState::InvalidColumnReference,
                message: "for SELECT DISTINCT, ORDER BY expressions must appear in select list"
                    .to_string(),
                position: None,
            },
        }
    }
}

impl From<SchemaError> for Error {
    fn from(error: SchemaError) -> Error {
        let (sqlstate, message) = match error {
            SchemaError::UndefinedTable { table } => (
                SqlState::UndefinedTable,
                format!("relation \"{table}\" does not exist"),
            ),
            SchemaError::DuplicateTable { table } => (
                SqlState::DuplicateTable,
                format!("relation \"{table}\" already exists"),
            ),
            SchemaError::UndefinedColumn { table, column } => (
                SqlState::UndefinedColumn,
                format!("column \"{column}\" of relation \"{table}\" does not exist"),
            ),
            SchemaError::DuplicateColumn { column } => (
                SqlState::DuplicateColumn,
                format!("column \"{column}\" specified more than once"),
            ),
            SchemaError::MultiplePrimaryKeys { table } => (
                SqlState::InvalidTableDefinition,
                format!("multiple primary keys for table \"{table}\" are not allowed"),
            ),
            SchemaError::UndefinedTableToDrop { table } => (
                SqlState::UndefinedTable,
                format!("table \"{table}\" does not exist"),
            ),
            SchemaError::DuplicateAssignment { column } => (
                SqlState::SyntaxError,
                format!("multiple assignments to same column \"{column}\""),
            ),
            SchemaError::TypeMismatch {
                column,
                expected,
                found,
            } => (
                SqlState::DatatypeMismatch,
                format!(
                    "column \"{column}\" is of type {expected} but expression is of type {found}"
                ),
            ),
            SchemaError::ValueTooLong { data_type } => (
                SqlState::StringDataRightTruncation,
                format!("value too long for type {data_type}"),
            ),
            SchemaError::OutOfRange => (
                SqlState::NumericValueOutOfRange,
                "integer out of range".to_string(),
            ),
            SchemaError::MoreValuesThanColumns => (
                SqlState::SyntaxError,
                "INSERT has more expressions than target columns".to_string(),
            ),
            SchemaError::MoreColumnsThanValues => (
                SqlState::SyntaxError,
                "INSERT has more target columns than expressions".to_string(),
            ),
        };
        Error {
            sqlstate,
            message,
            position: None,
        }
    }
}

impl From<ConstraintError> for Error {
    fn from(error: ConstraintError) -> Error {
        let (sqlstate, message) = match error {
            ConstraintError::NotNull { table, column } => (
                SqlState::NotNullViolation,
                format!(
                    "null value in column \"{column}\" of relation \"{table}\" violates not-null constraint"
                ),
            ),
            ConstraintError::Unique { constraint } => (
                SqlState::UniqueViolation,
                format!("duplicate key value violates unique constraint \"{constraint}\""),
            ),
        };
        Error {
            sqlstate,
            message,
            position: None,
        }
    }
}

impl From<PageError> for Error {
    fn from(error: PageError) -> Error {
        let (sqlstate, message) = match error {
            PageError::TupleTooLarge { size } => (
                SqlState::ProgramLimitExceeded,
                format!("row is too big: size {size}, maximum size {MAX_TUPLE_SIZE}"),
            ),
            PageError::PageFull => (SqlState::InternalError, "page is full".to_string()),
        };
        Error {
            sqlstate,
            message,
            position: None,
        }
    }
}

impl From<TupleError> for Error {
    fn from(error: TupleError) -> Error {
        Error {
            sqlstate: SqlState::DataCorrupted,
            message: format!("invalid tuple: {error:?}"),
            position: None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Error {
        Error {
            sqlstate: SqlState::IoError,
            message: format!("could not access file: {error}"),
            position: None,
        }
    }
}

impl From<HeapError> for Error {
    fn from(error: HeapError) -> Error {
        match error {
            HeapError::Page(error) => error.into(),
            HeapError::Tuple(error) => error.into(),
            HeapError::Io(error) => error.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::token::Token;

    #[test]
    fn each_sqlstate_has_its_code() {
        assert_eq!(SqlState::SyntaxError.code(), "42601");
        assert_eq!(SqlState::NumericValueOutOfRange.code(), "22003");
        assert_eq!(SqlState::DivisionByZero.code(), "22012");
        assert_eq!(SqlState::DatatypeMismatch.code(), "42804");
    }

    #[test]
    fn lexical_error_becomes_a_syntax_error_near_the_character() {
        let error = Error::from(LexError {
            position: 3,
            found: '?',
        });
        assert_eq!(error.sqlstate(), SqlState::SyntaxError);
        assert_eq!(error.message(), "syntax error at or near \"?\"");
        assert_eq!(error.position(), Some(3));
    }

    #[test]
    fn unexpected_token_becomes_a_syntax_error_near_the_token() {
        let error = Error::from(ParseError::UnexpectedToken {
            token: Token::String("it's".to_string()),
            position: 8,
        });
        assert_eq!(error.message(), "syntax error at or near \"'it''s'\"");
        assert_eq!(error.position(), Some(8));
    }

    #[test]
    fn unexpected_end_becomes_a_syntax_error_without_position() {
        let error = Error::from(ParseError::UnexpectedEnd);
        assert_eq!(error.message(), "syntax error at end of input");
        assert_eq!(error.position(), None);
    }

    #[test]
    fn evaluation_errors_have_their_own_sqlstates() {
        assert_eq!(
            Error::from(EvalError::NumericOutOfRange).sqlstate(),
            SqlState::NumericValueOutOfRange
        );
        assert_eq!(
            Error::from(EvalError::DivisionByZero).sqlstate(),
            SqlState::DivisionByZero
        );
        assert_eq!(
            Error::from(EvalError::DatatypeMismatch).sqlstate(),
            SqlState::DatatypeMismatch
        );
    }

    #[test]
    fn display_shows_the_message() {
        let error = Error::from(EvalError::DivisionByZero);
        assert_eq!(error.to_string(), "division by zero");
    }

    #[test]
    fn file_error_becomes_an_io_error() {
        let error = Error::from(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ));
        assert_eq!(error.sqlstate(), SqlState::IoError);
        assert_eq!(error.sqlstate().code(), "58030");
        assert_eq!(error.message(), "could not access file: denied");
    }
}
