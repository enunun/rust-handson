# Iteration 12：集約(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 12-1 準備

引き継いだ288個のテストがすべて通れば準備は終わりである．

## 12-2 文法と概念

課題の解答例である．

```rust
use std::collections::HashMap;

pub fn totals(rows: &[(&str, i64)]) -> Vec<(String, i64)> {
    let mut indexes: HashMap<&str, usize> = HashMap::new();
    let mut totals: Vec<(String, i64)> = Vec::new();
    for &(dept, salary) in rows {
        let index = *indexes.entry(dept).or_insert_with(|| {
            totals.push((dept.to_string(), 0));
            totals.len() - 1
        });
        totals[index].1 += salary;
    }
    totals
}

pub fn totals_with_null(rows: &[(Option<&str>, i64)]) -> Vec<(Option<String>, i64)> {
    let mut indexes: HashMap<Option<&str>, usize> = HashMap::new();
    let mut totals: Vec<(Option<String>, i64)> = Vec::new();
    for &(dept, salary) in rows {
        let index = *indexes.entry(dept).or_insert_with(|| {
            totals.push((dept.map(|dept| dept.to_string()), 0));
            totals.len() - 1
        });
        totals[index].1 += salary;
    }
    totals
}

pub trait Aggregate {
    fn add(&mut self, n: i64);
    fn finish(&self) -> Option<i64>;
}

#[derive(Default)]
struct Sum {
    total: Option<i64>,
}

#[derive(Default)]
struct Max {
    max: Option<i64>,
}

impl Aggregate for Sum {
    fn add(&mut self, n: i64) {
        self.total = Some(self.total.unwrap_or(0) + n);
    }
    fn finish(&self) -> Option<i64> {
        self.total
    }
}

impl Aggregate for Max {
    fn add(&mut self, n: i64) {
        self.max = Some(match self.max {
            Some(max) => max.max(n),
            None => n,
        });
    }
    fn finish(&self) -> Option<i64> {
        self.max
    }
}

pub fn aggregate(name: &str) -> Box<dyn Aggregate> {
    match name {
        "sum" => Box::new(Sum::default()),
        _ => Box::new(Max::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        assert_eq!(
            totals(&[("dev", 500), ("ops", 400), ("dev", 450)]),
            vec![("dev".to_string(), 950), ("ops".to_string(), 400)]
        );
        assert_eq!(
            totals_with_null(&[(None, 1), (Some("dev"), 2), (None, 3)]),
            vec![(None, 4), (Some("dev".to_string()), 2)]
        );
        let mut aggregates = [aggregate("sum"), aggregate("max")];
        for n in [3, 7, 5] {
            for aggregate in aggregates.iter_mut() {
                aggregate.add(n);
            }
        }
        let results: Vec<Option<i64>> = aggregates.iter().map(|a| a.finish()).collect();
        assert_eq!(results, vec![Some(15), Some(7)]);
    }
}
```

- `for &(dept, salary) in rows`は，要素への参照`&(&str, i64)`を，パターンで外して受け取る．
- `Option<&str>`も`Eq`と`Hash`を実装しているので，`HashMap`のキーにできる．`None`どうしは等しいので，1つのグループにまとまる．
- `totals[index].1`は，タプルの2つ目の要素である．
- `aggregates`を`vec!`で作ると，`cargo clippy`が配列で足りると指摘する．要素の数が決まっているなら，配列`[...]`を使う．

## 12-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 構文，名前解決，実行計画，`EXPLAIN`，`Filter`，集約の演算子，結合テストの順に並べた．
- 既存テストへの影響は，構文解析の`Select`の期待値(`group_by`と`having`)と，`Filter::new`の引数である．
- 集約関数の単体テストは，値がない場合，`NULL`を含む場合，負の数の平均，範囲を超える合計，型の誤りに分けた．
- 結合テストの表は，Iteration 11の`emp`にした．`dept`が`NULL`の社員がいるので，`NULL`のグループを確かめられる．

## 12-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | `exec::aggregate`と，`exec::build`からの依存，`exec::aggregate`からの依存を加えた | 集約の演算子ができた |
| `code-types.md` | `AggregateFunc`，`AggregateCall`，`BoundAggregate`，`HashAggregate`，`Accumulator`とその実装を加え，`Select`，`Expr`，`BoundSelect`，`PlanNode`，`Filter`，エラーの型を更新した | 集約を表す型ができた |
| `code-sequence.md` | 集約の図を加え，`SELECT`の演算子の重なり方の説明を更新した | グループに振り分けて集約した行を返す流れを示す |

- `Grouping`は`plan::binder`の中だけで使う型なので，型の図には描かなかった．
- `AggregateFunc`は構文木の型だが，名前解決と演算子でもそのまま使う．`JoinKind`と同じ考え方である．

## 12-5 テスト駆動の実装

### 集約関数，`GROUP BY`，`HAVING`の構文

```rust
#[test]
fn aggregate_calls() {
    assert_eq!(
        parse_sql("VALUES (COUNT(*), count(a), SUM(DISTINCT a + 1), AVG(a), MIN(a), MAX(a))"),
        Ok(vec![
            aggregate(AggregateFunc::Count, None, false),
            aggregate(AggregateFunc::Count, Some(column("A")), false),
            aggregate(
                AggregateFunc::Sum,
                Some(arithmetic(ArithmeticOp::Add, column("A"), int(1))),
                true
            ),
            aggregate(AggregateFunc::Avg, Some(column("A")), false),
            aggregate(AggregateFunc::Min, Some(column("A")), false),
            aggregate(AggregateFunc::Max, Some(column("A")), false),
        ])
    );
}
```

`operand`の選択肢に`aggregate_call`を加えた．

```rust
fn aggregate_call(input: &mut Tokens<'_>) -> ModalResult<Expr> {
    alt((
        (
            literal(keyword(Keyword::Count)),
            literal(Token::LParen),
            literal(Token::Star),
            cut_err(literal(Token::RParen)),
        )
            .map(|_| Expr::Aggregate {
                func: AggregateFunc::Count,
                arg: None,
                distinct: false,
            }),
        (
            aggregate_func,
            preceded(
                literal(Token::LParen),
                cut_err((
                    opt(literal(keyword(Keyword::Distinct))).map(|distinct| distinct.is_some()),
                    terminated(expr, literal(Token::RParen)),
                )),
            ),
        )
            .map(|(func, (distinct, arg))| Expr::Aggregate {
                func,
                arg: Some(Box::new(arg)),
                distinct,
            }),
    ))
    .parse_next(input)
}
```

1つ目の選択肢は，`COUNT ( *`のどこかで読めなければ戻る．`COUNT(a)`は，2つ目の選択肢で読み直す．
`GROUP BY`は`ORDER BY`と同じく，なければ空の並びを返すパーサーにした．既存の`Select`の期待値には，`group_by: vec![]`と`having: None`を加えた．

### 集約する問い合わせの名前解決

```rust
#[test]
fn grouped_select_items_refer_to_the_aggregated_row() {
    let bound =
        aggregate_of("SELECT name, COUNT(*), COUNT(*) + 1 FROM users GROUP BY name").unwrap();
    let aggregate = bound.aggregate.unwrap();
    assert_eq!(aggregate.keys, vec![BoundExpr::Column(1)]);
    // COUNT(*) は2度現れるが，calls には1つだけ入る
    assert_eq!(bound.items[0], BoundExpr::Column(0));
    assert_eq!(bound.items[1], BoundExpr::Column(1));
    assert_eq!(bound.input_columns, vec!["USERS.NAME", "COUNT(*)"]);
    assert_eq!(bound.names, vec!["NAME", "COUNT", "?column?"]);
}
```

集約する問い合わせでは，`Grouping`に`GROUP BY`のキーと，見つけた集約関数の呼び出しを集める．

```rust
    fn bind(&mut self, expr: &Expr, scope: &Scope) -> Result<BoundExpr, BindError> {
        if let Expr::Aggregate {
            func,
            arg,
            distinct,
        } = expr
        {
            let arg = match arg {
                Some(arg) => Some(bind(arg, scope).map_err(|error| match error {
                    BindError::MisplacedAggregate => BindError::NestedAggregate,
                    other => other,
                })?),
                None => None,
            };
            let call = AggregateCall {
                func: *func,
                arg,
                distinct: *distinct,
            };
            let index = match self.calls.iter().position(|known| *known == call) {
                Some(index) => index,
                None => {
                    self.calls.push(call);
                    self.calls.len() - 1
                }
            };
            return Ok(BoundExpr::Column(self.keys.len() + index));
        }
        if !expr.contains_aggregate() {
            let bound = bind(expr, scope)?;
            if let Some(key) = self.key(&bound) {
                return Ok(key);
            }
        }
        let bound = match expr {
            Expr::Column(name) => {
                return Err(BindError::NotGrouped {
                    column: name.clone(),
                });
            }
            // QualifiedColumn も NotGrouped，Unary，Binary，IsNull は子の式を self.bind で解決する
            // 定数は bind で定数にする
        };
        Ok(bound)
    }
```

- 集約関数の呼び出しは，集約した行の`キーの数 + 呼び出しの番号`の列になる．同じ呼び出しは，前に集めたものを使う．
- 集約関数を含まない式は，まず`FROM`の行について解決する．`GROUP BY`のキーと同じ式なら，キーの列にする．`GROUP BY dept`で`dept || '!'`を選ぶと，`dept`の部分がキーの列になる．
- キーでない列にたどり着いたら`NotGrouped`である．

`bind_select`は，`GROUP BY`か`HAVING`があるか，選択項目か`ORDER BY`に集約関数があれば，`Grouping`を作る．
選択項目，`HAVING`，`ORDER BY`は`bind_item`で解決する．`Grouping`があれば`Grouping::bind`，なければ`bind`を呼ぶ．
`input_columns`は，集約する問い合わせなら，集約した行の列名(キーと呼び出しの表示)にした．`EXPLAIN`の`Project`と`Filter`は，この名前で式を表示する．

### 集約関数を書けない場所

```rust
#[test]
fn aggregates_are_not_allowed_in_where_or_inside_aggregates() {
    assert_eq!(
        aggregate_of("SELECT id FROM users WHERE COUNT(*) > 1"),
        Err(BindError::AggregateNotAllowed { clause: "WHERE" })
    );
    assert_eq!(
        aggregate_of("SELECT SUM(COUNT(*)) FROM users"),
        Err(BindError::NestedAggregate)
    );
}
```

`bind`は，どの句を解決しているかを知らない．集約関数を見つけたら`MisplacedAggregate`を返し，呼んだ側が句の名前を付ける．

```rust
pub fn bind_in(expr: &Expr, scope: &Scope, clause: &'static str) -> Result<BoundExpr, BindError> {
    bind(expr, scope).map_err(|error| match error {
        BindError::MisplacedAggregate => BindError::AggregateNotAllowed { clause },
        other => other,
    })
}
```

`WHERE`，`ON`，`GROUP BY`は`bind_in`で解決する．`database`の`VALUES`，`INSERT`，`UPDATE`も`bind_in`に変えた．
集約関数の引数の中で見つけた`MisplacedAggregate`は，`NestedAggregate`に変える．

### 集約関数

```rust
#[test]
fn avg_truncates_toward_zero() {
    assert_eq!(
        finish(&call(AggregateFunc::Avg, false), &[int(-3), int(-4)]),
        Ok(Value::BigInt(-3))
    );
}
```

`src/exec/aggregate.rs`に`Accumulator`を定義し，`Count`，`Sum`，`Avg`，`Extreme`を1つずつ実装した．

```rust
pub trait Accumulator {
    fn add(&mut self, value: &Value) -> Result<(), EvalError>;
    fn finish(&self) -> Value;
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
```

- Rustの整数の`/`は0の方向に切り捨てるので，`-7 / 2`は`-3`である．SQLの要件と同じなので，そのまま使える．
- `integer`は，値が整数でなければ`UndefinedFunction`を返す．`SUM(name)`は，最初の`NULL`でない値を受け取った時点でエラーになる．
- `MIN`と`MAX`は，比べる向きだけが違うので，1つの型`Extreme`にまとめた．
- `DISTINCT`の項目で，受け取った値を`HashSet`で覚える`DistinctValues`を加えた．中に別の`Accumulator`を持ち，初めての値だけを渡す．

集約関数の呼び出しから`Accumulator`を作る関数は，`Box<dyn Accumulator>`を返す．

```rust
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
```

`inner`の型を`Box<dyn Accumulator>`と書いた．書かないと，最初の腕から`Box<Count>`と推論され，ほかの腕と型が合わない．

### `HashAggregate`

```rust
#[test]
fn groups_are_returned_in_the_order_they_first_appear() {
    let rows = vec![
        vec![int(2), int(10)],
        vec![Value::Null, int(5)],
        vec![int(1), int(20)],
        vec![int(2), int(30)],
        vec![Value::Null, int(7)],
    ];
    // COUNT(*) と SUM(2列目) を，1列目で GROUP BY する
    assert_eq!(
        aggregate(rows, vec![BoundExpr::Column(0)], vec![count_star, sum]),
        vec![
            vec![int(2), Value::BigInt(2), Value::BigInt(40)],
            vec![Value::Null, Value::BigInt(2), Value::BigInt(12)],
            vec![int(1), Value::BigInt(1), Value::BigInt(20)],
        ]
    );
}
```

```rust
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
```

- `entry(key.clone())`は，キーを`HashMap`に入れるための複製である．元の`key`は，`or_insert_with`のクロージャが`groups`に移す．
- `zip(&mut groups[index].1)`で，呼び出しとそのグループの`Accumulator`を組にして，`&mut`で`add`を呼ぶ．
- `COUNT(*)`は引数がないので，`NULL`でない値`TRUE`を渡す．`Count`は値を見ずに数える．
- キーがなく行もなければ，空のキーのグループを1つ作る．`COUNT(*)`は`0`になる．

### 実行計画と`HAVING`

```rust
#[test]
fn aggregate_and_having_are_between_the_scan_and_the_project() {
    let plan = plan_sql("SELECT name, COUNT(*) FROM users GROUP BY name HAVING COUNT(*) > 1");
    // Project の下に Filter(HAVING)，その下に HashAggregate がある
}
```

`plan`は，`WHERE`の`Filter`の上に`HashAggregate`を，その上に`HAVING`の`Filter`を置く．
`HAVING`の条件が真偽値でないときのメッセージのため，`PlanNode::Filter`と`Filter`に句の名前`clause`を加えた．既存の`Filter::new`の呼び出しには`"WHERE"`を渡すように直した．
`aggregate.rs`の残りの結合テストは，ここまでの実装で通る．

## 12-6 振り返り

1. 構文，名前解決，集約関数，`HashAggregate`，結合テストのそれぞれに，`NULL`と値がない場合の項目があるかを比べる．
2. 列挙型で持つと，`add`と`finish`は集約関数ごとの腕を持つ`match`になる．集約関数を加えるときは，列挙子と2つの`match`を変える．トレイトオブジェクトなら，新しい型に`Accumulator`を実装し，`accumulator`関数に1行加える．Iteration 10の`PlanNode`(列挙型)と`Executor`(トレイト)も，同じ選択である．`PlanNode`は種類ごとに扱いを変える処理(`EXPLAIN`，計画の組み立て)が多いので列挙型に，`Executor`は同じ操作`next`を持つ演算子を組み合わせるのでトレイトにした．
3. `HashMap`は要素の順序を決めないので，`HashMap`を順にたどって結果を作ると，実行のたびにグループの順序が変わりうる．テストの期待値も書けない．最初に現れた順で`Vec`へ置けば，順序が決まる．
4. 集約した行の列の並びは，`plan::binder`が決める(`Grouping`の`keys`と`calls`)．`plan::planner`は`BoundAggregate`をそのまま`HashAggregate`に渡し，`exec::aggregate`はキーの値のあとに結果を並べる．3つのモジュールが同じ並びを前提にしている．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 12-7 発展課題

解答例である．キーワード`FILTER`を加え，`Expr::Aggregate`と`AggregateCall`に`filter`を加える．

```rust
fn aggregate_call(input: &mut Tokens<'_>) -> ModalResult<Expr> {
    (
        alt((
            (
                literal(keyword(Keyword::Count)),
                literal(Token::LParen),
                literal(Token::Star),
                cut_err(literal(Token::RParen)),
            )
                .map(|_| (AggregateFunc::Count, None, false)),
            (
                aggregate_func,
                preceded(
                    literal(Token::LParen),
                    cut_err((
                        opt(literal(keyword(Keyword::Distinct))).map(|distinct| distinct.is_some()),
                        terminated(expr, literal(Token::RParen)),
                    )),
                ),
            )
                .map(|(func, (distinct, arg))| (func, Some(Box::new(arg)), distinct)),
        )),
        opt(preceded(
            literal(keyword(Keyword::Filter)),
            cut_err(delimited(
                (literal(Token::LParen), literal(keyword(Keyword::Where))),
                expr,
                literal(Token::RParen),
            )),
        )),
    )
        .map(|((func, arg, distinct), filter)| Expr::Aggregate {
            func,
            arg,
            distinct,
            filter: filter.map(Box::new),
        })
        .parse_next(input)
}
```

名前解決では，条件も引数と同じく`FROM`の行について解決する．`HashAggregate`は，条件が真でない行をその呼び出しに渡さない．

```rust
            for (call, accumulator) in self.calls.iter().zip(&mut groups[index].1) {
                let passes = match &call.filter {
                    Some(filter) => eval_condition(filter, &row, "FILTER")?,
                    None => true,
                };
                if !passes {
                    continue;
                }
                // 以下は引数を評価して add する
            }
```

`AggregateCall::display`は，条件があれば`COUNT(*) FILTER (WHERE (EMP.SALARY > 420))`のように書き添える．

```rust
#[test]
fn filter_clause_limits_the_rows_of_one_aggregate() {
    let result = query(
        &mut database(),
        "SELECT dept, COUNT(*), COUNT(*) FILTER (WHERE salary > 420) AS rich FROM emp GROUP BY dept",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![varchar("dev"), big(2), big(2)],
            vec![varchar("ops"), big(1), big(0)],
            vec![Value::Null, big(1), big(0)],
        ]
    );
}
```

`Expr::Aggregate`と`AggregateCall`を作っている既存のテストには，`filter: None`を加える．
