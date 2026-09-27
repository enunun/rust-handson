//! 2つの表を結合する演算子．

use crate::error::Error;
use crate::exec::Executor;
use crate::exec::eval::eval_condition;
use crate::plan::binder::BoundExpr;
use crate::sql::ast::JoinKind;
use crate::value::{Row, Value};

/// 入れ子ループ結合．外側(左)の行ごとに，内側(右)のすべての行と組にする．
/// 組の行は，外側の行の値のあとに内側の行の値を並べたものである．
/// 内側の行は，最初の`next`ですべて読んで手元に置き，外側の行ごとに先頭から読み直す．
pub struct NestedLoopJoin {
    left: Box<dyn Executor>,
    right: Box<dyn Executor>,
    kind: JoinKind,
    condition: Option<BoundExpr>,
    right_width: usize,
    right_rows: Option<Vec<Row>>,
    current: Option<Row>,
    position: usize,
    matched: bool,
}

impl NestedLoopJoin {
    pub fn new(
        left: Box<dyn Executor>,
        right: Box<dyn Executor>,
        kind: JoinKind,
        condition: Option<BoundExpr>,
        right_width: usize,
    ) -> NestedLoopJoin {
        NestedLoopJoin {
            left,
            right,
            kind,
            condition,
            right_width,
            right_rows: None,
            current: None,
            position: 0,
            matched: false,
        }
    }

    /// 内側の行をすべて読む．
    fn read_right(&mut self) -> Result<Vec<Row>, Error> {
        let mut rows = Vec::new();
        while let Some(row) = self.right.next()? {
            rows.push(row);
        }
        Ok(rows)
    }

    /// 組の行が`ON`の条件を満たすかを返す．条件がなければ(`CROSS JOIN`)，常に満たす．
    fn satisfies(&self, row: &[Value]) -> Result<bool, Error> {
        match &self.condition {
            Some(condition) => Ok(eval_condition(condition, row, "JOIN/ON")?),
            None => Ok(true),
        }
    }
}

impl Executor for NestedLoopJoin {
    fn next(&mut self) -> Result<Option<Row>, Error> {
        if self.right_rows.is_none() {
            let rows = self.read_right()?;
            self.right_rows = Some(rows);
        }
        loop {
            let Some(left) = &self.current else {
                match self.left.next()? {
                    Some(row) => {
                        self.current = Some(row);
                        self.position = 0;
                        self.matched = false;
                        continue;
                    }
                    None => return Ok(None),
                }
            };
            let Some(right_rows) = &self.right_rows else {
                unreachable!("the inner rows are read before the loop");
            };
            while self.position < right_rows.len() {
                let mut row = left.clone();
                row.extend_from_slice(&right_rows[self.position]);
                self.position += 1;
                if self.satisfies(&row)? {
                    self.matched = true;
                    return Ok(Some(row));
                }
            }
            let left = self.current.take().expect("the current outer row exists");
            if self.kind == JoinKind::Left && !self.matched {
                let mut row = left;
                row.extend(std::iter::repeat_n(Value::Null, self.right_width));
                return Ok(Some(row));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::build::collect_rows;
    use crate::exec::scan::SeqScan;
    use crate::sql::ast::{BinaryOp, ComparisonOp};

    fn int(n: i32) -> Value {
        Value::Integer(n)
    }

    fn scan(values: &[i32]) -> Box<dyn Executor> {
        Box::new(SeqScan::new(values.iter().map(|&n| vec![int(n)]).collect()))
    }

    /// 左の1列目と右の1列目が等しいという条件．
    fn equal() -> Option<BoundExpr> {
        Some(BoundExpr::Binary {
            op: BinaryOp::Comparison(ComparisonOp::Eq),
            left: Box::new(BoundExpr::Column(0)),
            right: Box::new(BoundExpr::Column(1)),
        })
    }

    fn join(left: &[i32], right: &[i32], kind: JoinKind, condition: Option<BoundExpr>) -> Vec<Row> {
        let mut join = NestedLoopJoin::new(scan(left), scan(right), kind, condition, 1);
        collect_rows(&mut join).unwrap()
    }

    #[test]
    fn cross_join_pairs_every_left_row_with_every_right_row() {
        assert_eq!(
            join(&[1, 2], &[10, 20], JoinKind::Cross, None),
            vec![
                vec![int(1), int(10)],
                vec![int(1), int(20)],
                vec![int(2), int(10)],
                vec![int(2), int(20)],
            ]
        );
    }

    #[test]
    fn inner_join_returns_pairs_that_satisfy_the_condition() {
        assert_eq!(
            join(&[1, 2, 3], &[3, 1, 1], JoinKind::Inner, equal()),
            vec![
                vec![int(1), int(1)],
                vec![int(1), int(1)],
                vec![int(3), int(3)],
            ]
        );
    }

    #[test]
    fn left_join_pads_unmatched_left_rows_with_null() {
        assert_eq!(
            join(&[1, 2], &[2], JoinKind::Left, equal()),
            vec![vec![int(1), Value::Null], vec![int(2), int(2)]]
        );
        assert_eq!(
            join(&[1], &[], JoinKind::Left, equal()),
            vec![vec![int(1), Value::Null]]
        );
    }
}
