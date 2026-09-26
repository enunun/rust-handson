/// SQLの字句(トークン)．
#[derive(Debug, Clone, PartialEq)]
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
}

/// SQLのキーワード．
#[derive(Debug, Clone, PartialEq)]
pub enum Keyword {
    Values,
}
