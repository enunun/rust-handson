use std::fmt;

/// SQLの字句(トークン)．
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Keyword(Keyword),
    Identifier(String),
    Integer(i64),
    String(String),
    LParen,
    RParen,
    Comma,
    Dot,
    Plus,
    Minus,
    Star,
    Slash,
    Concat,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

/// SQLのキーワード．
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyword {
    Values,
    Create,
    Table,
    Insert,
    Into,
    Select,
    From,
    Where,
    As,
    Update,
    Set,
    Delete,
    Drop,
    Primary,
    Key,
    Unique,
    Order,
    By,
    Asc,
    Desc,
    Nulls,
    First,
    Last,
    Offset,
    Rows,
    Fetch,
    Only,
    Distinct,
    Explain,
    Join,
    Cross,
    Inner,
    Left,
    Outer,
    On,
    Integer,
    Int,
    Bigint,
    Boolean,
    Varchar,
    True,
    False,
    Unknown,
    Null,
    And,
    Or,
    Not,
    Is,
}

/// SQLの文の中の位置を付けた値．`position`は値の先頭の文字の位置で，先頭の文字を1と数える．
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub position: usize,
}

/// 位置を付けたトークンを，トークンそのものと比べられるようにする．
/// winnowの`literal(Token::Comma)`で，位置を付けたトークンの列を読むために使う．
impl PartialEq<Token> for Spanned<Token> {
    fn eq(&self, other: &Token) -> bool {
        self.value == *other
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Keyword(keyword) => write!(f, "{keyword}"),
            Token::Identifier(name) => f.write_str(name),
            Token::Integer(n) => write!(f, "{n}"),
            Token::String(s) => write!(f, "'{}'", s.replace('\'', "''")),
            Token::LParen => f.write_str("("),
            Token::RParen => f.write_str(")"),
            Token::Comma => f.write_str(","),
            Token::Dot => f.write_str("."),
            Token::Plus => f.write_str("+"),
            Token::Minus => f.write_str("-"),
            Token::Star => f.write_str("*"),
            Token::Slash => f.write_str("/"),
            Token::Concat => f.write_str("||"),
            Token::Eq => f.write_str("="),
            Token::NotEq => f.write_str("<>"),
            Token::Lt => f.write_str("<"),
            Token::LtEq => f.write_str("<="),
            Token::Gt => f.write_str(">"),
            Token::GtEq => f.write_str(">="),
        }
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let word = match self {
            Keyword::Values => "VALUES",
            Keyword::Create => "CREATE",
            Keyword::Table => "TABLE",
            Keyword::Insert => "INSERT",
            Keyword::Into => "INTO",
            Keyword::Select => "SELECT",
            Keyword::From => "FROM",
            Keyword::Where => "WHERE",
            Keyword::As => "AS",
            Keyword::Update => "UPDATE",
            Keyword::Set => "SET",
            Keyword::Delete => "DELETE",
            Keyword::Drop => "DROP",
            Keyword::Primary => "PRIMARY",
            Keyword::Key => "KEY",
            Keyword::Unique => "UNIQUE",
            Keyword::Order => "ORDER",
            Keyword::By => "BY",
            Keyword::Asc => "ASC",
            Keyword::Desc => "DESC",
            Keyword::Nulls => "NULLS",
            Keyword::First => "FIRST",
            Keyword::Last => "LAST",
            Keyword::Offset => "OFFSET",
            Keyword::Rows => "ROWS",
            Keyword::Fetch => "FETCH",
            Keyword::Only => "ONLY",
            Keyword::Distinct => "DISTINCT",
            Keyword::Explain => "EXPLAIN",
            Keyword::Join => "JOIN",
            Keyword::Cross => "CROSS",
            Keyword::Inner => "INNER",
            Keyword::Left => "LEFT",
            Keyword::Outer => "OUTER",
            Keyword::On => "ON",
            Keyword::Integer => "INTEGER",
            Keyword::Int => "INT",
            Keyword::Bigint => "BIGINT",
            Keyword::Boolean => "BOOLEAN",
            Keyword::Varchar => "VARCHAR",
            Keyword::True => "TRUE",
            Keyword::False => "FALSE",
            Keyword::Unknown => "UNKNOWN",
            Keyword::Null => "NULL",
            Keyword::And => "AND",
            Keyword::Or => "OR",
            Keyword::Not => "NOT",
            Keyword::Is => "IS",
        };
        f.write_str(word)
    }
}
