//! 重複する行を除く演算子．

use crate::error::Error;
use crate::exec::Executor;
use crate::value::Row;

/// 子の演算子の行のうち，それまでに返していない行だけを返す．`NULL`どうしは同じ値とみなす．
pub struct Distinct {
    input: Box<dyn Executor>,
    seen: Vec<Row>,
}

impl Distinct {
    pub fn new(input: Box<dyn Executor>) -> Distinct {
        Distinct {
            input,
            seen: Vec::new(),
        }
    }
}

impl Executor for Distinct {
    fn next(&mut self) -> Result<Option<Row>, Error> {
        while let Some(row) = self.input.next()? {
            if !self.seen.contains(&row) {
                self.seen.push(row.clone());
                return Ok(Some(row));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::build::collect_rows;
    use crate::exec::scan::SeqScan;
    use crate::value::Value;

    fn int(n: i32) -> Value {
        Value::Integer(n)
    }

    fn scan(rows: Vec<Row>) -> Box<dyn Executor> {
        Box::new(SeqScan::new(rows))
    }

    #[test]
    fn distinct_returns_each_row_once_in_the_first_order() {
        let input = scan(vec![
            vec![int(2)],
            vec![Value::Null],
            vec![int(2)],
            vec![int(1)],
            vec![Value::Null],
        ]);
        let mut distinct = Distinct::new(input);
        assert_eq!(
            collect_rows(&mut distinct),
            Ok(vec![vec![int(2)], vec![Value::Null], vec![int(1)]])
        );
    }
}
