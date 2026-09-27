//! 文の実行に使う処理．`SELECT`は，演算子(`Executor`)の木で実行する．

pub mod aggregate;
pub mod build;
pub mod distinct;
pub mod dml;
pub mod eval;
pub mod filter;
pub mod join;
pub mod limit;
pub mod project;
pub mod scan;
pub mod sort;

use crate::error::Error;
use crate::value::Row;

/// 実行計画の演算子を実行するもの．`next`を呼ぶたびに1行を返し，行がなくなれば`None`を返す．
pub trait Executor {
    fn next(&mut self) -> Result<Option<Row>, Error>;
}
