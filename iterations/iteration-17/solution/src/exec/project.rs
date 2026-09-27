//! 行から式の値を計算する演算子．

use crate::error::Error;
use crate::exec::Executor;
use crate::exec::eval::eval;
use crate::plan::binder::BoundExpr;
use crate::value::Row;

/// 子の演算子の各行について式を評価し，その値の並びを行として返す．
pub struct Project {
    input: Box<dyn Executor>,
    exprs: Vec<BoundExpr>,
}

impl Project {
    pub fn new(input: Box<dyn Executor>, exprs: Vec<BoundExpr>) -> Project {
        Project { input, exprs }
    }
}

impl Executor for Project {
    fn next(&mut self) -> Result<Option<Row>, Error> {
        match self.input.next()? {
            Some(row) => {
                let values = self
                    .exprs
                    .iter()
                    .map(|expr| eval(expr, &row))
                    .collect::<Result<Row, _>>()?;
                Ok(Some(values))
            }
            None => Ok(None),
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

    #[test]
    fn project_computes_each_expression_for_each_row() {
        let input = scan(vec![vec![int(1), int(10)], vec![int(2), int(20)]]);
        let exprs = vec![BoundExpr::Column(1), BoundExpr::Constant(int(0))];
        let mut project = Project::new(input, exprs);
        assert_eq!(
            collect_rows(&mut project),
            Ok(vec![vec![int(10), int(0)], vec![int(20), int(0)]])
        );
    }
}
