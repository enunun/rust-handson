//! 表の行を順に返す演算子．

use crate::error::Error;
use crate::exec::Executor;
use crate::value::Row;

/// 表のすべての行を，表に入っている順に返す．
pub struct SeqScan {
    rows: std::vec::IntoIter<Row>,
}

impl SeqScan {
    pub fn new(rows: Vec<Row>) -> SeqScan {
        SeqScan {
            rows: rows.into_iter(),
        }
    }
}

impl Executor for SeqScan {
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
    fn seq_scan_returns_every_row_in_order() {
        let rows = vec![vec![Value::Integer(1)], vec![Value::Integer(2)]];
        let mut scan = SeqScan::new(rows.clone());
        assert_eq!(collect_rows(&mut scan), Ok(rows));
        assert_eq!(scan.next(), Ok(None));
    }
}
