use ferrodb::{Keyword, LexError, Token, tokenize};

#[test]
fn tokenizes_a_values_statement() {
    assert_eq!(
        tokenize("values (1, 2 + 3)"),
        Ok(vec![
            Token::Keyword(Keyword::Values),
            Token::LParen,
            Token::Integer(1),
            Token::Comma,
            Token::Integer(2),
            Token::Plus,
            Token::Integer(3),
            Token::RParen,
        ])
    );
}

#[test]
fn reports_the_position_of_an_unknown_character() {
    assert_eq!(tokenize("VALUES (1 ? 2)"), Err(LexError { position: 11 }));
}
