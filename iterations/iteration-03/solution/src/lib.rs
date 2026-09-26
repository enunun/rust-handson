//! SQLを処理するデータベース`ferrodb`のライブラリ．

mod ast;
mod eval;
mod lexer;
mod parser;
mod token;
mod value;

pub use eval::EvalError;
pub use lexer::{LexError, tokenize};
pub use parser::ParseError;
pub use token::{Keyword, Token};
pub use value::Value;

/// SQLの実行のエラー．どの段階で失敗したかを列挙子で表す．
#[derive(Debug, PartialEq)]
pub enum Error {
    Lex(LexError),
    Parse(ParseError),
    Eval(EvalError),
}

/// 問い合わせの結果の表．
#[derive(Debug, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// SQLの文を1つ実行し，結果の表を返す．
pub fn execute(sql: &str) -> Result<QueryResult, Error> {
    let tokens = tokenize(sql).map_err(Error::Lex)?;
    let values = parser::parse(&tokens).map_err(Error::Parse)?;
    let mut columns = Vec::new();
    let mut row = Vec::new();
    for expr in &values.exprs {
        columns.push(format!("COLUMN{}", columns.len() + 1));
        row.push(eval::eval(expr).map_err(Error::Eval)?);
    }
    Ok(QueryResult {
        columns,
        rows: vec![row],
    })
}
