//! 行をグループにまとめ，集約関数の結果を計算する演算子．

use std::collections::{HashMap, HashSet};

use crate::error::Error;
use crate::exec::Executor;
use crate::exec::eval::{EvalError, eval};
use crate::plan::binder::{AggregateCall, BoundExpr};
use crate::sql::ast::AggregateFunc;
use crate::value::{Row, Value};

/// 1つのグループの，1つの集約関数の途中の結果．`NULL`でない値を1つずつ受け取る．
pub trait Accumulator {
    fn add(&mut self, value: &Value) -> Result<(), EvalError>;
    fn finish(&self) -> Value;
}

/// `COUNT`．受け取った値の数を`BIGINT`で返す．
#[derive(Default)]
struct Count {
    count: i64,
}

/// `SUM`．整数の合計を`BIGINT`で返す．値がなければ`NULL`を返す．
#[derive(Default)]
struct Sum {
    total: Option<i64>,
}

/// `AVG`．整数の平均を0の方向に切り捨てた`BIGINT`で返す．値がなければ`NULL`を返す．
#[derive(Default)]
struct Avg {
    total: i64,
    count: i64,
}

/// `MIN`と`MAX`．`Value`の順序で最小か最大の値を返す．値がなければ`NULL`を返す．
struct Extreme {
    best: Option<Value>,
    max: bool,
}

/// `DISTINCT`付きの集約．まだ受け取っていない値だけを`inner`に渡す．
struct DistinctValues {
    seen: HashSet<Value>,
    inner: Box<dyn Accumulator>,
}

impl Accumulator for Count {
    fn add(&mut self, _value: &Value) -> Result<(), EvalError> {
        self.count += 1;
        Ok(())
    }

    fn finish(&self) -> Value {
        Value::BigInt(self.count)
    }
}

impl Accumulator for Sum {
    fn add(&mut self, value: &Value) -> Result<(), EvalError> {
        let n = integer("SUM", value)?;
        let total = self.total.unwrap_or(0);
        self.total = Some(total.checked_add(n).ok_or(EvalError::NumericOutOfRange)?);
        Ok(())
    }

    fn finish(&self) -> Value {
        match self.total {
            Some(total) => Value::BigInt(total),
            None => Value::Null,
        }
    }
}

impl Accumulator for Avg {
    fn add(&mut self, value: &Value) -> Result<(), EvalError> {
        let n = integer("AVG", value)?;
        self.total = self
            .total
            .checked_add(n)
            .ok_or(EvalError::NumericOutOfRange)?;
        self.count += 1;
        Ok(())
    }

    fn finish(&self) -> Value {
        if self.count == 0 {
            Value::Null
        } else {
            Value::BigInt(self.total / self.count)
        }
    }
}

impl Accumulator for Extreme {
    fn add(&mut self, value: &Value) -> Result<(), EvalError> {
        let replace = match &self.best {
            None => true,
            Some(best) if self.max => value > best,
            Some(best) => value < best,
        };
        if replace {
            self.best = Some(value.clone());
        }
        Ok(())
    }

    fn finish(&self) -> Value {
        self.best.clone().unwrap_or(Value::Null)
    }
}

impl Accumulator for DistinctValues {
    fn add(&mut self, value: &Value) -> Result<(), EvalError> {
        if self.seen.insert(value.clone()) {
            self.inner.add(value)?;
        }
        Ok(())
    }

    fn finish(&self) -> Value {
        self.inner.finish()
    }
}

/// 整数の値を`i64`として読む．整数でなければ，その型の引数を取る関数がないというエラーを返す．
fn integer(name: &'static str, value: &Value) -> Result<i64, EvalError> {
    match value {
        Value::Integer(n) => Ok(i64::from(*n)),
        Value::BigInt(n) => Ok(*n),
        other => Err(EvalError::UndefinedFunction {
            name,
            argument: other.type_name(),
        }),
    }
}

/// 集約関数の呼び出しに対応する，空の`Accumulator`を作る．
fn accumulator(call: &AggregateCall) -> Box<dyn Accumulator> {
    let inner: Box<dyn Accumulator> = match call.func {
        AggregateFunc::Count => Box::new(Count::default()),
        AggregateFunc::Sum => Box::new(Sum::default()),
        AggregateFunc::Avg => Box::new(Avg::default()),
        AggregateFunc::Min => Box::new(Extreme {
            best: None,
            max: false,
        }),
        AggregateFunc::Max => Box::new(Extreme {
            best: None,
            max: true,
        }),
    };
    if call.distinct {
        Box::new(DistinctValues {
            seen: HashSet::new(),
            inner,
        })
    } else {
        inner
    }
}

/// 子の演算子の行をすべて読み，キーの値でグループにまとめる．グループごとに，キーの値のあとに
/// 集約関数の結果を並べた行を，グループが最初に現れた順で返す．
/// キーがなければ，行がなくてもグループを1つ作る．
pub struct HashAggregate {
    input: Box<dyn Executor>,
    keys: Vec<BoundExpr>,
    calls: Vec<AggregateCall>,
    results: Option<std::vec::IntoIter<Row>>,
}

impl HashAggregate {
    pub fn new(
        input: Box<dyn Executor>,
        keys: Vec<BoundExpr>,
        calls: Vec<AggregateCall>,
    ) -> HashAggregate {
        HashAggregate {
            input,
            keys,
            calls,
            results: None,
        }
    }

    fn new_group(&self) -> Vec<Box<dyn Accumulator>> {
        self.calls.iter().map(accumulator).collect()
    }

    /// 子の演算子の行をすべて読み，グループごとの結果の行を作る．
    fn aggregate(&mut self) -> Result<Vec<Row>, Error> {
        let mut indexes: HashMap<Row, usize> = HashMap::new();
        let mut groups: Vec<(Row, Vec<Box<dyn Accumulator>>)> = Vec::new();
        while let Some(row) = self.input.next()? {
            let key = self
                .keys
                .iter()
                .map(|key| eval(key, &row))
                .collect::<Result<Row, _>>()?;
            let index = *indexes.entry(key.clone()).or_insert_with(|| {
                groups.push((key, self.new_group()));
                groups.len() - 1
            });
            for (call, accumulator) in self.calls.iter().zip(&mut groups[index].1) {
                let value = match &call.arg {
                    Some(arg) => eval(arg, &row)?,
                    None => Value::Boolean(true),
                };
                if !value.is_null() {
                    accumulator.add(&value)?;
                }
            }
        }
        if groups.is_empty() && self.keys.is_empty() {
            groups.push((Vec::new(), self.new_group()));
        }
        Ok(groups
            .into_iter()
            .map(|(mut key, accumulators)| {
                key.extend(accumulators.iter().map(|accumulator| accumulator.finish()));
                key
            })
            .collect())
    }
}

impl Executor for HashAggregate {
    fn next(&mut self) -> Result<Option<Row>, Error> {
        if self.results.is_none() {
            let rows = self.aggregate()?;
            self.results = Some(rows.into_iter());
        }
        match &mut self.results {
            Some(rows) => Ok(rows.next()),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::build::collect_rows;
    use crate::exec::scan::SeqScan;

    fn int(n: i32) -> Value {
        Value::Integer(n)
    }

    fn call(func: AggregateFunc, distinct: bool) -> AggregateCall {
        AggregateCall {
            func,
            arg: Some(BoundExpr::Column(0)),
            distinct,
        }
    }

    /// 値を1つずつ加えた集約関数の結果．`NULL`は`HashAggregate`と同じく加えない．
    fn finish(call: &AggregateCall, values: &[Value]) -> Result<Value, EvalError> {
        let mut accumulator = accumulator(call);
        for value in values.iter().filter(|value| !value.is_null()) {
            accumulator.add(value)?;
        }
        Ok(accumulator.finish())
    }

    #[test]
    fn aggregates_of_no_values() {
        assert_eq!(
            finish(&call(AggregateFunc::Count, false), &[]),
            Ok(Value::BigInt(0))
        );
        for func in [
            AggregateFunc::Sum,
            AggregateFunc::Avg,
            AggregateFunc::Min,
            AggregateFunc::Max,
        ] {
            assert_eq!(finish(&call(func, false), &[Value::Null]), Ok(Value::Null));
        }
    }

    #[test]
    fn aggregates_ignore_null() {
        let values = [int(3), Value::Null, int(1), int(4)];
        let result = |func| finish(&call(func, false), &values);
        assert_eq!(result(AggregateFunc::Count), Ok(Value::BigInt(3)));
        assert_eq!(result(AggregateFunc::Sum), Ok(Value::BigInt(8)));
        assert_eq!(result(AggregateFunc::Avg), Ok(Value::BigInt(2)));
        assert_eq!(result(AggregateFunc::Min), Ok(int(1)));
        assert_eq!(result(AggregateFunc::Max), Ok(int(4)));
    }

    #[test]
    fn avg_truncates_toward_zero() {
        assert_eq!(
            finish(&call(AggregateFunc::Avg, false), &[int(-3), int(-4)]),
            Ok(Value::BigInt(-3))
        );
    }

    #[test]
    fn sum_beyond_bigint_is_out_of_range() {
        assert_eq!(
            finish(
                &call(AggregateFunc::Sum, false),
                &[Value::BigInt(i64::MAX), int(1)]
            ),
            Err(EvalError::NumericOutOfRange)
        );
    }

    #[test]
    fn sum_of_strings_is_an_undefined_function() {
        assert_eq!(
            finish(
                &call(AggregateFunc::Sum, false),
                &[Value::Varchar("a".to_string())]
            ),
            Err(EvalError::UndefinedFunction {
                name: "SUM",
                argument: "character varying"
            })
        );
    }

    #[test]
    fn min_and_max_compare_strings() {
        let values = [
            Value::Varchar("b".to_string()),
            Value::Varchar("a".to_string()),
        ];
        assert_eq!(
            finish(&call(AggregateFunc::Min, false), &values),
            Ok(Value::Varchar("a".to_string()))
        );
    }

    #[test]
    fn distinct_aggregate_uses_each_value_once() {
        let values = [int(2), int(2), int(3)];
        assert_eq!(
            finish(&call(AggregateFunc::Count, true), &values),
            Ok(Value::BigInt(2))
        );
        assert_eq!(
            finish(&call(AggregateFunc::Sum, true), &values),
            Ok(Value::BigInt(5))
        );
    }

    fn aggregate(rows: Vec<Row>, keys: Vec<BoundExpr>, calls: Vec<AggregateCall>) -> Vec<Row> {
        let mut aggregate = HashAggregate::new(Box::new(SeqScan::new(rows)), keys, calls);
        collect_rows(&mut aggregate).unwrap()
    }

    #[test]
    fn groups_are_returned_in_the_order_they_first_appear() {
        let rows = vec![
            vec![int(2), int(10)],
            vec![Value::Null, int(5)],
            vec![int(1), int(20)],
            vec![int(2), int(30)],
            vec![Value::Null, int(7)],
        ];
        let count_star = AggregateCall {
            func: AggregateFunc::Count,
            arg: None,
            distinct: false,
        };
        let sum = AggregateCall {
            func: AggregateFunc::Sum,
            arg: Some(BoundExpr::Column(1)),
            distinct: false,
        };
        assert_eq!(
            aggregate(rows, vec![BoundExpr::Column(0)], vec![count_star, sum]),
            vec![
                vec![int(2), Value::BigInt(2), Value::BigInt(40)],
                vec![Value::Null, Value::BigInt(2), Value::BigInt(12)],
                vec![int(1), Value::BigInt(1), Value::BigInt(20)],
            ]
        );
    }

    #[test]
    fn no_keys_make_one_group_even_without_rows() {
        let count_star = AggregateCall {
            func: AggregateFunc::Count,
            arg: None,
            distinct: false,
        };
        assert_eq!(
            aggregate(vec![], vec![], vec![count_star.clone()]),
            vec![vec![Value::BigInt(0)]]
        );
        assert_eq!(
            aggregate(vec![], vec![BoundExpr::Column(0)], vec![count_star]),
            Vec::<Row>::new()
        );
    }
}
