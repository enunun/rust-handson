use std::ops::Range;

use winnow::Parser;
use winnow::ascii::{digit1, multispace0};
use winnow::combinator::{alt, delimited, preceded, repeat, terminated};
use winnow::stream::LocatingSlice;
use winnow::token::{none_of, one_of, take_while};

use crate::sql::token::{Keyword, Spanned, Token};

/// 字句解析のエラー．`position`は解釈できなかった文字`found`の位置で，先頭の文字を1と数える．
/// バイトではなく文字の数で数える．
#[derive(Debug, PartialEq)]
pub struct LexError {
    pub position: usize,
    pub found: char,
}

type Input<'i> = LocatingSlice<&'i str>;

/// SQLの文字列を，位置を付けたトークンの列に分ける．
pub fn tokenize(sql: &str) -> Result<Vec<Spanned<Token>>, LexError> {
    match tokens.parse(LocatingSlice::new(sql)) {
        Ok(tokens) => {
            let mut spanned = Vec::new();
            for (token, span) in tokens {
                spanned.push(Spanned {
                    value: token,
                    position: char_position(sql, span.start),
                });
            }
            Ok(spanned)
        }
        Err(error) => {
            let rest = &sql[error.offset()..];
            Err(LexError {
                position: char_position(sql, error.offset()),
                found: rest
                    .chars()
                    .next()
                    .expect("the lexer stops only before an unreadable character"),
            })
        }
    }
}

/// バイトの位置`byte`にある文字が，先頭から何文字目か(1から数える)を返す．
fn char_position(sql: &str, byte: usize) -> usize {
    sql[..byte].chars().count() + 1
}

fn tokens(input: &mut Input<'_>) -> winnow::Result<Vec<(Token, Range<usize>)>> {
    terminated(
        repeat(0.., preceded(multispace0, token.with_span())),
        multispace0,
    )
    .parse_next(input)
}

fn token(input: &mut Input<'_>) -> winnow::Result<Token> {
    alt((word, quoted_identifier, integer, string, comparison, symbol)).parse_next(input)
}

/// キーワードか，引用符で囲まない識別子を読む．識別子は大文字に正規化する．
fn word(input: &mut Input<'_>) -> winnow::Result<Token> {
    (
        one_of(('a'..='z', 'A'..='Z', '_')),
        take_while(0.., ('a'..='z', 'A'..='Z', '0'..='9', '_')),
    )
        .take()
        .map(to_word_token)
        .parse_next(input)
}

fn to_word_token(word: &str) -> Token {
    let upper = word.to_ascii_uppercase();
    match to_keyword(&upper) {
        Some(keyword) => Token::Keyword(keyword),
        None => Token::Identifier(upper),
    }
}

fn to_keyword(word: &str) -> Option<Keyword> {
    match word {
        "VALUES" => Some(Keyword::Values),
        "CREATE" => Some(Keyword::Create),
        "TABLE" => Some(Keyword::Table),
        "INSERT" => Some(Keyword::Insert),
        "INTO" => Some(Keyword::Into),
        "SELECT" => Some(Keyword::Select),
        "FROM" => Some(Keyword::From),
        "WHERE" => Some(Keyword::Where),
        "AS" => Some(Keyword::As),
        "UPDATE" => Some(Keyword::Update),
        "SET" => Some(Keyword::Set),
        "DELETE" => Some(Keyword::Delete),
        "DROP" => Some(Keyword::Drop),
        "PRIMARY" => Some(Keyword::Primary),
        "KEY" => Some(Keyword::Key),
        "UNIQUE" => Some(Keyword::Unique),
        "ORDER" => Some(Keyword::Order),
        "BY" => Some(Keyword::By),
        "ASC" => Some(Keyword::Asc),
        "DESC" => Some(Keyword::Desc),
        "NULLS" => Some(Keyword::Nulls),
        "FIRST" => Some(Keyword::First),
        "LAST" => Some(Keyword::Last),
        "OFFSET" => Some(Keyword::Offset),
        "ROWS" => Some(Keyword::Rows),
        "FETCH" => Some(Keyword::Fetch),
        "ONLY" => Some(Keyword::Only),
        "DISTINCT" => Some(Keyword::Distinct),
        "EXPLAIN" => Some(Keyword::Explain),
        "JOIN" => Some(Keyword::Join),
        "CROSS" => Some(Keyword::Cross),
        "INNER" => Some(Keyword::Inner),
        "LEFT" => Some(Keyword::Left),
        "OUTER" => Some(Keyword::Outer),
        "ON" => Some(Keyword::On),
        "COUNT" => Some(Keyword::Count),
        "SUM" => Some(Keyword::Sum),
        "AVG" => Some(Keyword::Avg),
        "MIN" => Some(Keyword::Min),
        "MAX" => Some(Keyword::Max),
        "GROUP" => Some(Keyword::Group),
        "HAVING" => Some(Keyword::Having),
        "INTEGER" => Some(Keyword::Integer),
        "INT" => Some(Keyword::Int),
        "BIGINT" => Some(Keyword::Bigint),
        "BOOLEAN" => Some(Keyword::Boolean),
        "VARCHAR" => Some(Keyword::Varchar),
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

/// 二重引用符で囲んだ識別子を読む．大文字と小文字をそのまま残す．
fn quoted_identifier(input: &mut Input<'_>) -> winnow::Result<Token> {
    delimited('"', repeat(0.., quoted_identifier_char), '"')
        .map(Token::Identifier)
        .parse_next(input)
}

fn quoted_identifier_char(input: &mut Input<'_>) -> winnow::Result<char> {
    alt(("\"\"".value('"'), none_of('"'))).parse_next(input)
}

fn integer(input: &mut Input<'_>) -> winnow::Result<Token> {
    digit1.parse_to().map(Token::Integer).parse_next(input)
}

fn symbol(input: &mut Input<'_>) -> winnow::Result<Token> {
    alt((
        '('.value(Token::LParen),
        ')'.value(Token::RParen),
        ','.value(Token::Comma),
        '.'.value(Token::Dot),
        '+'.value(Token::Plus),
        '-'.value(Token::Minus),
        '*'.value(Token::Star),
        '/'.value(Token::Slash),
        "||".value(Token::Concat),
    ))
    .parse_next(input)
}

fn string(input: &mut Input<'_>) -> winnow::Result<Token> {
    delimited('\'', repeat(0.., string_char), '\'')
        .map(Token::String)
        .parse_next(input)
}

fn string_char(input: &mut Input<'_>) -> winnow::Result<char> {
    alt(("''".value('\''), none_of('\''))).parse_next(input)
}

fn comparison(input: &mut Input<'_>) -> winnow::Result<Token> {
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

    fn kinds(sql: &str) -> Result<Vec<Token>, LexError> {
        let mut kinds = Vec::new();
        for token in tokenize(sql)? {
            kinds.push(token.value);
        }
        Ok(kinds)
    }

    #[test]
    fn tokens_carry_their_character_positions() {
        let mut positions = Vec::new();
        for token in tokenize("VALUES ('あ', 1)").unwrap() {
            positions.push(token.position);
        }
        assert_eq!(positions, vec![1, 8, 9, 12, 14, 15]);
    }

    #[test]
    fn empty_input_gives_no_tokens() {
        assert_eq!(kinds(""), Ok(vec![]));
    }

    #[test]
    fn digits_become_an_integer() {
        assert_eq!(kinds("42"), Ok(vec![Token::Integer(42)]));
    }

    #[test]
    fn arithmetic_operators() {
        assert_eq!(
            kinds("+-*/"),
            Ok(vec![Token::Plus, Token::Minus, Token::Star, Token::Slash])
        );
    }

    #[test]
    fn parentheses_and_comma() {
        assert_eq!(
            kinds("(,)"),
            Ok(vec![Token::LParen, Token::Comma, Token::RParen])
        );
    }

    #[test]
    fn values_is_a_keyword() {
        assert_eq!(kinds("VALUES"), Ok(vec![Token::Keyword(Keyword::Values)]));
    }

    #[test]
    fn keywords_are_case_insensitive() {
        assert_eq!(kinds("values"), Ok(vec![Token::Keyword(Keyword::Values)]));
        assert_eq!(kinds("Values"), Ok(vec![Token::Keyword(Keyword::Values)]));
    }

    #[test]
    fn comparison_operators() {
        assert_eq!(
            kinds("= <> < <= > >="),
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
        assert_eq!(kinds("<>1"), Ok(vec![Token::NotEq, Token::Integer(1)]));
        assert_eq!(kinds("<=1"), Ok(vec![Token::LtEq, Token::Integer(1)]));
    }

    #[test]
    fn boolean_and_null_keywords() {
        assert_eq!(
            kinds("true false unknown null and or not is"),
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
        assert_eq!(kinds("'abc'"), Ok(vec![Token::String("abc".to_string())]));
    }

    #[test]
    fn empty_string_literal() {
        assert_eq!(kinds("''"), Ok(vec![Token::String(String::new())]));
    }

    #[test]
    fn doubled_quote_is_one_quote() {
        assert_eq!(
            kinds("'it''s'"),
            Ok(vec![Token::String("it's".to_string())])
        );
    }

    #[test]
    fn string_keeps_spaces_and_non_ascii_characters() {
        assert_eq!(
            kinds("' 日本 語 '"),
            Ok(vec![Token::String(" 日本 語 ".to_string())])
        );
    }

    #[test]
    fn unterminated_string_reports_the_opening_quote() {
        assert_eq!(
            kinds("1 'abc"),
            Err(LexError {
                position: 3,
                found: '\''
            })
        );
    }

    #[test]
    fn concatenation_operator() {
        assert_eq!(
            kinds("'a'||'b'"),
            Ok(vec![
                Token::String("a".to_string()),
                Token::Concat,
                Token::String("b".to_string())
            ])
        );
    }

    #[test]
    fn position_counts_characters_not_bytes() {
        assert_eq!(
            kinds("'あ' ?"),
            Err(LexError {
                position: 5,
                found: '?'
            })
        );
    }

    #[test]
    fn whitespace_and_newlines_are_skipped() {
        assert_eq!(
            kinds(" 1 +\n2 "),
            Ok(vec![Token::Integer(1), Token::Plus, Token::Integer(2)])
        );
    }

    #[test]
    fn unknown_character_reports_its_position() {
        assert_eq!(
            kinds("1 ? 2"),
            Err(LexError {
                position: 3,
                found: '?'
            })
        );
    }

    #[test]
    fn word_that_is_not_a_keyword_is_an_identifier() {
        assert_eq!(
            kinds("1 users"),
            Ok(vec![
                Token::Integer(1),
                Token::Identifier("USERS".to_string())
            ])
        );
    }

    #[test]
    fn identifier_may_contain_digits_and_underscores() {
        assert_eq!(
            kinds("_user_2"),
            Ok(vec![Token::Identifier("_USER_2".to_string())])
        );
    }

    #[test]
    fn keyword_followed_by_letters_is_an_identifier() {
        assert_eq!(
            kinds("VALUESX"),
            Ok(vec![Token::Identifier("VALUESX".to_string())])
        );
    }

    #[test]
    fn quoted_identifier_keeps_its_case() {
        assert_eq!(
            kinds("\"Users\""),
            Ok(vec![Token::Identifier("Users".to_string())])
        );
    }

    #[test]
    fn quoted_identifier_may_be_a_keyword_and_contain_quotes() {
        assert_eq!(
            kinds("\"select\" \"a\"\"b\""),
            Ok(vec![
                Token::Identifier("select".to_string()),
                Token::Identifier("a\"b".to_string())
            ])
        );
    }

    #[test]
    fn statement_keywords() {
        assert_eq!(
            kinds("create table insert into select from"),
            Ok(vec![
                Token::Keyword(Keyword::Create),
                Token::Keyword(Keyword::Table),
                Token::Keyword(Keyword::Insert),
                Token::Keyword(Keyword::Into),
                Token::Keyword(Keyword::Select),
                Token::Keyword(Keyword::From),
            ])
        );
    }

    #[test]
    fn data_type_keywords() {
        assert_eq!(
            kinds("integer int bigint boolean varchar"),
            Ok(vec![
                Token::Keyword(Keyword::Integer),
                Token::Keyword(Keyword::Int),
                Token::Keyword(Keyword::Bigint),
                Token::Keyword(Keyword::Boolean),
                Token::Keyword(Keyword::Varchar),
            ])
        );
    }

    #[test]
    fn largest_i64_is_an_integer() {
        assert_eq!(
            kinds("9223372036854775807"),
            Ok(vec![Token::Integer(i64::MAX)])
        );
    }

    #[test]
    fn integer_beyond_i64_is_an_error() {
        assert_eq!(
            kinds("9223372036854775808"),
            Err(LexError {
                position: 1,
                found: '9'
            })
        );
    }

    #[test]
    fn dot_separates_a_table_and_a_column() {
        assert_eq!(
            kinds("e.name"),
            Ok(vec![
                Token::Identifier("E".to_string()),
                Token::Dot,
                Token::Identifier("NAME".to_string())
            ])
        );
    }
}
