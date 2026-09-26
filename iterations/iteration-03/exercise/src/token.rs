/// SQLの字句(トークン)．
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Keyword(Keyword),
    Integer(i64),
    LParen,
    RParen,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
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
