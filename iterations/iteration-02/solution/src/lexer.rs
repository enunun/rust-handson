use winnow::Parser;
use winnow::ascii::{alpha1, digit1, multispace0};
use winnow::combinator::{alt, preceded, repeat, terminated};

use crate::token::{Keyword, Token};

/// 字句解析のエラー．`position`は解釈できなかった文字の位置で，先頭の文字を1と数える．
#[derive(Debug, PartialEq)]
pub struct LexError {
    pub position: usize,
}

/// SQLの文字列をトークンの列に分ける．
pub fn tokenize(sql: &str) -> Result<Vec<Token>, LexError> {
    match tokens.parse(sql) {
        Ok(tokens) => Ok(tokens),
        Err(error) => Err(LexError {
            position: error.offset() + 1,
        }),
    }
}

fn tokens(input: &mut &str) -> winnow::Result<Vec<Token>> {
    terminated(repeat(0.., preceded(multispace0, token)), multispace0).parse_next(input)
}

fn token(input: &mut &str) -> winnow::Result<Token> {
    alt((keyword, integer, comparison, symbol)).parse_next(input)
}

fn keyword(input: &mut &str) -> winnow::Result<Token> {
    alpha1
        .verify_map(to_keyword)
        .map(Token::Keyword)
        .parse_next(input)
}

fn to_keyword(word: &str) -> Option<Keyword> {
    match word.to_ascii_uppercase().as_str() {
        "VALUES" => Some(Keyword::Values),
        "TRUE" => Some(Keyword::True),
        "FALSE" => Some(Keyword::False),
        "UNKNOWN" => Some(Keyword::Unknown),
        "NULL" => Some(Keyword::Null),
        "AND" => Some(Keyword::And),
        "OR" => Some(Keyword::Or),
        "NOT" => Some(Keyword::Not),
        "IS" => Some(Keyword::Is),
        _ => None,
    }
}

fn integer(input: &mut &str) -> winnow::Result<Token> {
    digit1.parse_to().map(Token::Integer).parse_next(input)
}

fn symbol(input: &mut &str) -> winnow::Result<Token> {
    alt((
        '('.value(Token::LParen),
        ')'.value(Token::RParen),
        ','.value(Token::Comma),
        '+'.value(Token::Plus),
        '-'.value(Token::Minus),
        '*'.value(Token::Star),
        '/'.value(Token::Slash),
    ))
    .parse_next(input)
}

fn comparison(input: &mut &str) -> winnow::Result<Token> {
    alt((
        '='.value(Token::Eq),
        "<>".value(Token::NotEq),
        "<=".value(Token::LtEq),
        '<'.value(Token::Lt),
        ">=".value(Token::GtEq),
        '>'.value(Token::Gt),
    ))
    .parse_next(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_gives_no_tokens() {
        assert_eq!(tokenize(""), Ok(vec![]));
    }

    #[test]
    fn digits_become_an_integer() {
        assert_eq!(tokenize("42"), Ok(vec![Token::Integer(42)]));
    }

    #[test]
    fn arithmetic_operators() {
        assert_eq!(
            tokenize("+-*/"),
            Ok(vec![Token::Plus, Token::Minus, Token::Star, Token::Slash])
        );
    }

    #[test]
    fn parentheses_and_comma() {
        assert_eq!(
            tokenize("(,)"),
            Ok(vec![Token::LParen, Token::Comma, Token::RParen])
        );
    }

    #[test]
    fn values_is_a_keyword() {
        assert_eq!(
            tokenize("VALUES"),
            Ok(vec![Token::Keyword(Keyword::Values)])
        );
    }

    #[test]
    fn keywords_are_case_insensitive() {
        assert_eq!(
            tokenize("values"),
            Ok(vec![Token::Keyword(Keyword::Values)])
        );
        assert_eq!(
            tokenize("Values"),
            Ok(vec![Token::Keyword(Keyword::Values)])
        );
    }

    #[test]
    fn comparison_operators() {
        assert_eq!(
            tokenize("= <> < <= > >="),
            Ok(vec![
                Token::Eq,
                Token::NotEq,
                Token::Lt,
                Token::LtEq,
                Token::Gt,
                Token::GtEq
            ])
        );
    }

    #[test]
    fn two_character_operators_are_read_as_one_token() {
        assert_eq!(tokenize("<>1"), Ok(vec![Token::NotEq, Token::Integer(1)]));
        assert_eq!(tokenize("<=1"), Ok(vec![Token::LtEq, Token::Integer(1)]));
    }

    #[test]
    fn boolean_and_null_keywords() {
        assert_eq!(
            tokenize("true false unknown null and or not is"),
            Ok(vec![
                Token::Keyword(Keyword::True),
                Token::Keyword(Keyword::False),
                Token::Keyword(Keyword::Unknown),
                Token::Keyword(Keyword::Null),
                Token::Keyword(Keyword::And),
                Token::Keyword(Keyword::Or),
                Token::Keyword(Keyword::Not),
                Token::Keyword(Keyword::Is),
            ])
        );
    }

    #[test]
    fn whitespace_and_newlines_are_skipped() {
        assert_eq!(
            tokenize(" 1 +\n2 "),
            Ok(vec![Token::Integer(1), Token::Plus, Token::Integer(2)])
        );
    }

    #[test]
    fn unknown_character_reports_its_position() {
        assert_eq!(tokenize("1 ? 2"), Err(LexError { position: 3 }));
    }

    #[test]
    fn unknown_word_reports_its_first_character() {
        assert_eq!(tokenize("1 SELECT"), Err(LexError { position: 3 }));
    }

    #[test]
    fn largest_i64_is_an_integer() {
        assert_eq!(
            tokenize("9223372036854775807"),
            Ok(vec![Token::Integer(i64::MAX)])
        );
    }

    #[test]
    fn integer_beyond_i64_is_an_error() {
        assert_eq!(
            tokenize("9223372036854775808"),
            Err(LexError { position: 1 })
        );
    }
}
