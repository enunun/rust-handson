//! 返す行の数を制限する演算子．

use crate::error::Error;
use crate::exec::Executor;
use crate::value::Row;

/// 子の演算子の先頭の`offset`行を飛ばし，`fetch`行まで返す．`fetch`が`None`なら残りをすべて返す．
pub struct Limit {
    input: Box<dyn Executor>,
    offset: usize,
    remaining: Option<usize>,
}

impl Limit {
    pub fn new(input: Box<dyn Executor>, offset: usize, fetch: Option<usize>) -> Limit {
        Limit {
            input,
            offset,
            remaining: fetch,
        }
    }
}

impl Executor for Limit {
    fn next(&mut self) -> Result<Option<Row>, Error> {
        while self.offset > 0 {
            if self.input.next()?.is_none() {
                return Ok(None);
            }
            self.offset -= 1;
        }
        match self.remaining {
            Some(0) => Ok(None),
            Some(remaining) => {
                self.remaining = Some(remaining - 1);
                self.input.next()
            }
            None => self.input.next(),
        }
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

    fn numbers() -> Box<dyn Executor> {
        scan((1..=5).map(|n| vec![int(n)]).collect())
    }

    #[test]
    fn limit_skips_offset_rows_and_returns_fetch_rows() {
        let mut limit = Limit::new(numbers(), 1, Some(2));
        assert_eq!(
            collect_rows(&mut limit),
            Ok(vec![vec![int(2)], vec![int(3)]])
        );
    }

    #[test]
    fn limit_without_fetch_returns_the_rest() {
        let mut limit = Limit::new(numbers(), 3, None);
        assert_eq!(
            collect_rows(&mut limit),
            Ok(vec![vec![int(4)], vec![int(5)]])
        );
    }

    #[test]
    fn limit_beyond_the_rows_returns_no_rows() {
        let mut limit = Limit::new(numbers(), 9, Some(1));
        assert_eq!(collect_rows(&mut limit), Ok(vec![]));
        let mut limit = Limit::new(numbers(), 0, Some(0));
        assert_eq!(collect_rows(&mut limit), Ok(vec![]));
    }
}
