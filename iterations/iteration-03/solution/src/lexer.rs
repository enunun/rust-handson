use winnow::Parser;
use winnow::ascii::{alpha1, digit1, multispace0};
use winnow::combinator::{alt, delimited, preceded, repeat, terminated};
use winnow::token::none_of;

use crate::token::{Keyword, Token};

/// 字句解析のエラー．`position`は解釈できなかった文字の位置で，先頭の文字を1と数える．
/// バイトではなく文字の数で数える．
#[derive(Debug, PartialEq)]
pub struct LexError {
    pub position: usize,
}

/// SQLの文字列をトークンの列に分ける．
pub fn tokenize(sql: &str) -> Result<Vec<Token>, LexError> {
    match tokens.parse(sql) {
        Ok(tokens) => Ok(tokens),
        Err(error) => Err(LexError {
            position: sql[..error.offset()].chars().count() + 1,
        }),
    }
}

fn tokens(input: &mut &str) -> winnow::Result<Vec<Token>> {
    terminated(repeat(0.., preceded(multispace0, token)), multispace0).parse_next(input)
}

fn token(input: &mut &str) -> winnow::Result<Token> {
    alt((keyword, integer, string, comparison, symbol)).parse_next(input)
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
        "||".value(Token::Concat),
    ))
    .parse_next(input)
}

fn string(input: &mut &str) -> winnow::Result<Token> {
    delimited('\'', repeat(0.., string_char), '\'')
        .map(Token::String)
        .parse_next(input)
}

fn string_char(input: &mut &str) -> winnow::Result<char> {
    alt(("''".value('\''), none_of('\''))).parse_next(input)
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
    fn string_literal() {
        assert_eq!(
            tokenize("'abc'"),
            Ok(vec![Token::String("abc".to_string())])
        );
    }

    #[test]
    fn empty_string_literal() {
        assert_eq!(tokenize("''"), Ok(vec![Token::String(String::new())]));
    }

    #[test]
    fn doubled_quote_is_one_quote() {
        assert_eq!(
            tokenize("'it''s'"),
            Ok(vec![Token::String("it's".to_string())])
        );
    }

    #[test]
    fn string_keeps_spaces_and_non_ascii_characters() {
        assert_eq!(
            tokenize("' 日本 語 '"),
            Ok(vec![Token::String(" 日本 語 ".to_string())])
        );
    }

    #[test]
    fn unterminated_string_reports_the_opening_quote() {
        assert_eq!(tokenize("1 'abc"), Err(LexError { position: 3 }));
    }

    #[test]
    fn concatenation_operator() {
        assert_eq!(
            tokenize("'a'||'b'"),
            Ok(vec![
                Token::String("a".to_string()),
                Token::Concat,
                Token::String("b".to_string())
            ])
        );
    }

    #[test]
    fn position_counts_characters_not_bytes() {
        assert_eq!(tokenize("'あ' ?"), Err(LexError { position: 5 }));
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
