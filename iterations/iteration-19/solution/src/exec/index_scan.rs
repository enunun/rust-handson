//! インデックスで選んだ行を返す演算子．

use crate::error::Error;
use crate::exec::Executor;
use crate::value::Row;

/// インデックスのキーの範囲にある行を，キーの順に返す．
pub struct IndexScan {
    rows: std::vec::IntoIter<Row>,
}

impl IndexScan {
    /// `rows`は，インデックスのキーの順に並べた行である．
    pub fn new(rows: Vec<Row>) -> IndexScan {
        IndexScan {
            rows: rows.into_iter(),
        }
    }
}

impl Executor for IndexScan {
    fn next(&mut self) -> Result<Option<Row>, Error> {
        Ok(self.rows.next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::build::collect_rows;
    use crate::value::Value;

    #[test]
    fn index_scan_returns_the_given_rows_in_order() {
        let rows = vec![vec![Value::Integer(2)], vec![Value::Integer(1)]];
        let mut scan = IndexScan::new(rows.clone());
        assert_eq!(collect_rows(&mut scan), Ok(rows));
        assert_eq!(scan.next(), Ok(None));
    }
}
