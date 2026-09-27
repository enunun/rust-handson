//! 条件で行を絞り込む演算子．

use crate::error::Error;
use crate::exec::Executor;
use crate::exec::eval::eval_condition;
use crate::plan::binder::BoundExpr;
use crate::value::Row;

/// 子の演算子の行のうち，`WHERE`や`HAVING`の条件が真の行だけを返す．
/// `clause`は，条件が真偽値でないときのエラーに使う句の名前である．
pub struct Filter {
    input: Box<dyn Executor>,
    predicate: BoundExpr,
    clause: &'static str,
}

impl Filter {
    pub fn new(input: Box<dyn Executor>, predicate: BoundExpr, clause: &'static str) -> Filter {
        Filter {
            input,
            predicate,
            clause,
        }
    }
}

impl Executor for Filter {
    fn next(&mut self) -> Result<Option<Row>, Error> {
        while let Some(row) = self.input.next()? {
            if eval_condition(&self.predicate, &row, self.clause)? {
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
    use crate::sql::ast::{BinaryOp, ComparisonOp};
    use crate::value::Value;

    fn int(n: i32) -> Value {
        Value::Integer(n)
    }

    fn scan(rows: Vec<Row>) -> Box<dyn Executor> {
        Box::new(SeqScan::new(rows))
    }

    #[test]
    fn filter_returns_rows_where_the_condition_is_true() {
        let predicate = BoundExpr::Binary {
            op: BinaryOp::Comparison(ComparisonOp::Gt),
            left: Box::new(BoundExpr::Column(0)),
            right: Box::new(BoundExpr::Constant(int(1))),
        };
        let input = scan(vec![
            vec![int(1)],
            vec![int(3)],
            vec![Value::Null],
            vec![int(2)],
        ]);
        let mut filter = Filter::new(input, predicate, "WHERE");
        assert_eq!(
            collect_rows(&mut filter),
            Ok(vec![vec![int(3)], vec![int(2)]])
        );
    }
}
