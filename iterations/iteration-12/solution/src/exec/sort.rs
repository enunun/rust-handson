//! 行を`ORDER BY`のキーで並べ替える演算子．

use std::cmp::Ordering;

use crate::error::Error;
use crate::exec::Executor;
use crate::exec::eval::eval;
use crate::plan::binder::SortOrder;
use crate::plan::planner::SortKey;
use crate::value::{Row, Value};

/// 子の演算子の行をすべて読み，キーで並べ替えてから1行ずつ返す．
pub struct Sort {
    input: Box<dyn Executor>,
    keys: Vec<SortKey>,
    sorted: Option<std::vec::IntoIter<Row>>,
}

/// 行と，その行の並べ替えのキーの値．
#[derive(Debug, PartialEq)]
struct KeyedRow {
    values: Row,
    keys: Row,
}

impl Sort {
    pub fn new(input: Box<dyn Executor>, keys: Vec<SortKey>) -> Sort {
        Sort {
            input,
            keys,
            sorted: None,
        }
    }

    /// 子の演算子の行をすべて読み，キーの値を求めて並べ替える．
    fn read_and_sort(&mut self) -> Result<Vec<Row>, Error> {
        let mut rows = Vec::new();
        while let Some(row) = self.input.next()? {
            let keys = self
                .keys
                .iter()
                .map(|key| eval(&key.expr, &row))
                .collect::<Result<Row, _>>()?;
            rows.push(KeyedRow { values: row, keys });
        }
        let orders: Vec<SortOrder> = self.keys.iter().map(|key| key.order).collect();
        sort(&mut rows, &orders);
        Ok(rows.into_iter().map(|row| row.values).collect())
    }
}

impl Executor for Sort {
    fn next(&mut self) -> Result<Option<Row>, Error> {
        if self.sorted.is_none() {
            let rows = self.read_and_sort()?;
            self.sorted = Some(rows.into_iter());
        }
        match &mut self.sorted {
            Some(rows) => Ok(rows.next()),
            None => Ok(None),
        }
    }
}

/// 行をキーの値で並べ替える．すべてのキーが等しい行は，元の順序を保つ．
fn sort(rows: &mut [KeyedRow], orders: &[SortOrder]) {
    rows.sort_by(|a, b| compare_keys(&a.keys, &b.keys, orders));
}

/// キーの値の並びを，先頭のキーから順に比べる．前のキーが等しいときだけ，次のキーを比べる．
fn compare_keys(a: &[Value], b: &[Value], orders: &[SortOrder]) -> Ordering {
    let mut ordering = Ordering::Equal;
    for ((a, b), order) in a.iter().zip(b).zip(orders) {
        ordering = ordering.then_with(|| compare(a, b, order));
    }
    ordering
}

/// 1つのキーの値を，向きと`NULL`の位置に従って比べる．
fn compare(a: &Value, b: &Value, order: &SortOrder) -> Ordering {
    match (a.is_null(), b.is_null()) {
        (true, true) => Ordering::Equal,
        (true, false) if order.nulls_first => Ordering::Less,
        (true, false) => Ordering::Greater,
        (false, true) if order.nulls_first => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) if order.descending => a.cmp(b).reverse(),
        (false, false) => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::build::collect_rows;
    use crate::exec::scan::SeqScan;
    use crate::plan::binder::BoundExpr;

    fn keyed(keys: Vec<Value>) -> KeyedRow {
        KeyedRow {
            values: vec![],
            keys,
        }
    }

    fn sorted(keys: Vec<Vec<Value>>, orders: &[SortOrder]) -> Vec<Row> {
        let mut rows: Vec<KeyedRow> = keys.into_iter().map(keyed).collect();
        sort(&mut rows, orders);
        rows.into_iter().map(|row| row.keys).collect()
    }

    fn int(n: i32) -> Value {
        Value::Integer(n)
    }

    #[test]
    fn nulls_are_last_ascending_and_first_descending_by_default() {
        let keys = vec![vec![int(2)], vec![Value::Null], vec![int(1)]];
        assert_eq!(
            sorted(keys.clone(), &[SortOrder::new(false, None)]),
            vec![vec![int(1)], vec![int(2)], vec![Value::Null]]
        );
        assert_eq!(
            sorted(keys, &[SortOrder::new(true, None)]),
            vec![vec![Value::Null], vec![int(2)], vec![int(1)]]
        );
    }

    #[test]
    fn nulls_first_and_nulls_last_override_the_default() {
        let keys = vec![vec![int(2)], vec![Value::Null], vec![int(1)]];
        assert_eq!(
            sorted(keys.clone(), &[SortOrder::new(false, Some(true))]),
            vec![vec![Value::Null], vec![int(1)], vec![int(2)]]
        );
        assert_eq!(
            sorted(keys, &[SortOrder::new(true, Some(false))]),
            vec![vec![int(2)], vec![int(1)], vec![Value::Null]]
        );
    }

    #[test]
    fn later_keys_break_ties_of_earlier_keys() {
        let keys = vec![
            vec![int(1), int(1)],
            vec![int(2), int(5)],
            vec![int(1), int(3)],
        ];
        let orders = [SortOrder::new(false, None), SortOrder::new(true, None)];
        assert_eq!(
            sorted(keys, &orders),
            vec![
                vec![int(1), int(3)],
                vec![int(1), int(1)],
                vec![int(2), int(5)],
            ]
        );
    }

    #[test]
    fn rows_with_equal_keys_keep_their_order() {
        let mut rows = vec![
            KeyedRow {
                values: vec![int(1)],
                keys: vec![int(0)],
            },
            KeyedRow {
                values: vec![int(2)],
                keys: vec![int(0)],
            },
        ];
        sort(&mut rows, &[SortOrder::new(true, None)]);
        assert_eq!(rows[0].values, vec![int(1)]);
    }

    #[test]
    fn sort_returns_the_input_rows_in_key_order() {
        let input = SeqScan::new(vec![vec![int(2)], vec![Value::Null], vec![int(1)]]);
        let keys = vec![SortKey {
            expr: BoundExpr::Column(0),
            order: SortOrder::new(true, Some(false)),
        }];
        let mut sort = Sort::new(Box::new(input), keys);
        assert_eq!(
            collect_rows(&mut sort),
            Ok(vec![vec![int(2)], vec![int(1)], vec![Value::Null]])
        );
    }
}
