/// SQLの字句(トークン)．
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Keyword(Keyword),
    Integer(i64),
    String(String),
    LParen,
    RParen,
    Comma,
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
    True,
    False,
    Unknown,
    Null,
    And,
    Or,
    Not,
    Is,
}
