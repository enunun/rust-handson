//! SQLを処理するデータベース`ferrodb`のライブラリ．

mod ast;
mod error;
mod eval;
mod lexer;
mod parser;
mod token;
mod value;

pub use error::{Error, SqlState};
pub use lexer::{LexError, tokenize};
pub use token::{Keyword, Spanned, Token};
pub use value::Value;

/// 問い合わせの結果の表．
#[derive(Debug, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// SQLの文を1つ実行し，結果の表を返す．
pub fn execute(sql: &str) -> Result<QueryResult, Error> {
    let tokens = tokenize(sql)?;
    let values = parser::parse(&tokens)?;
    let mut columns = Vec::new();
    let mut row = Vec::new();
    for expr in &values.exprs {
        columns.push(format!("COLUMN{}", columns.len() + 1));
        row.push(eval::eval(expr)?);
    }
    Ok(QueryResult {
        columns,
        rows: vec![row],
    })
}
