use ferrodb::{Keyword, LexError, Spanned, Token, tokenize};

fn at(value: Token, position: usize) -> Spanned<Token> {
    Spanned { value, position }
}

#[test]
fn tokenizes_a_values_statement() {
    assert_eq!(
        tokenize("values (1, 2 + 3)"),
        Ok(vec![
            at(Token::Keyword(Keyword::Values), 1),
            at(Token::LParen, 8),
            at(Token::Integer(1), 9),
            at(Token::Comma, 10),
            at(Token::Integer(2), 12),
            at(Token::Plus, 14),
            at(Token::Integer(3), 16),
            at(Token::RParen, 17),
        ])
    );
}

#[test]
fn reports_the_position_of_an_unknown_character() {
    assert_eq!(
        tokenize("VALUES (1 ? 2)"),
        Err(LexError {
            position: 11,
            found: '?'
        })
    );
}
