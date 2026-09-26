# Iteration 10：実行計画とEXPLAIN(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 10-1 準備

引き継いだ236個のテストがすべて通れば準備は終わりである．

## 10-2 文法と概念

課題の解答例である．

```rust
pub trait Shape {
    fn area(&self) -> u32;
}

pub struct Rect {
    pub width: u32,
    pub height: u32,
}

pub struct Square {
    pub side: u32,
}

impl Shape for Rect {
    fn area(&self) -> u32 {
        self.width * self.height
    }
}

impl Shape for Square {
    fn area(&self) -> u32 {
        self.side * self.side
    }
}

pub fn total_area(shapes: &[Box<dyn Shape>]) -> u32 {
    shapes.iter().map(|shape| shape.area()).sum()
}

pub fn total_area_of<S: Shape>(shapes: &[S]) -> u32 {
    shapes.iter().map(|shape| shape.area()).sum()
}

pub trait Source {
    fn next(&mut self) -> Option<i32>;
}

pub struct Range {
    pub current: i32,
    pub end: i32,
}

impl Source for Range {
    fn next(&mut self) -> Option<i32> {
        if self.current >= self.end {
            return None;
        }
        self.current += 1;
        Some(self.current - 1)
    }
}

pub struct Skip {
    pub input: Box<dyn Source>,
    pub count: usize,
}

impl Source for Skip {
    fn next(&mut self) -> Option<i32> {
        while self.count > 0 {
            self.input.next()?;
            self.count -= 1;
        }
        self.input.next()
    }
}

pub fn collect_all(source: &mut dyn Source) -> Vec<i32> {
    let mut values = Vec::new();
    while let Some(n) = source.next() {
        values.push(n);
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let shapes: Vec<Box<dyn Shape>> = vec![
            Box::new(Rect {
                width: 2,
                height: 3,
            }),
            Box::new(Square { side: 4 }),
        ];
        assert_eq!(total_area(&shapes), 22);
        assert_eq!(total_area_of(&[Square { side: 1 }, Square { side: 2 }]), 5);

        let mut skip = Skip {
            input: Box::new(Range { current: 0, end: 5 }),
            count: 2,
        };
        assert_eq!(collect_all(&mut skip), vec![2, 3, 4]);
    }
}
```

- `sum`は，イテレーターの要素の合計を返す．
- `Skip`の`self.input.next()?`は，`Option`に対する`?`である．`None`なら，その場で`None`を返す．`Result`の`?`と同じ形で使える．
- `shapes`の型`Vec<Box<dyn Shape>>`は，変数に書いておく．書かないと，`vec!`の最初の要素から`Vec<Box<Rect>>`と推論され，`Square`を入れられない．

課題2で長方形と正方形を混ぜると，次のエラーになる．

```text
error[E0308]: mismatched types
  --> src/lib.rs:31:51
   |
31 |     total_area_of(&[Rect { width: 2, height: 3 }, Square { side: 4 }])
   |                                                   ^^^^^^^^^^^^^^^^^^ expected `Rect`, found `Square`
```

ジェネリクスの`S`は1つの型に決まる．配列の最初の要素から`S = Rect`と決まり，`Square`は入らない．

## 10-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 構文，名前解決，実行計画，`EXPLAIN`，演算子，結合テストの順に並べた．演算子は1つのモジュールに1つずつ置き，それぞれに単体テストを書いた．
- 演算子の単体テストでは，子に`SeqScan`を置いて行を流し込む．`Filter`には`NULL`の行を，`Distinct`には2つの`NULL`の行を入れた．
- `EXPLAIN`の結合テストでは，演算子を動かさないことを`1 / 0`で確かめた．`Project`が式を評価すれば0による除算になる．
- 引き継いだ`select`と`order_by`の結合テストは，リファクタリングの安全網である．期待値は1つも変えない．

## 10-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | `plan::planner`，`plan::explain`，`exec`，`exec::build`と演算子のモジュールを加えた．`database`から`exec::sort`への依存を除き，`plan::planner`，`plan::explain`，`exec::build`への依存を加えた | `SELECT`の処理を演算子に分けた |
| `code-types.md` | 図を2つに分け，2つ目に`BoundSelect`，`BoundOrderBy`，`SortSource`，`SortOrder`，`PlanNode`，`SortKey`，`Executor`と演算子の型を描いた | 名前解決と実行計画の型ができた |
| `code-sequence.md` | `SELECT`の図を，演算子が`next`で1行ずつ親へ渡す流れにした | Volcanoモデルを示す |

- `exec`は，トレイト`Executor`を定義するので，名前空間でなく1つのComponentになった．演算子のモジュールはどれも`exec`に依存する．
- 構文木の`Limit`と演算子の`Limit`は，同じ名前の別の型である．`classDiagram`のクラス名は1つの図の中で重ねられないので，型の図を2つに分けた．
- 照合スクリプトが求める依存を描くと，演算子のモジュールから`error`と`value`への線が多くなる．演算子ごとの役割は，図の下の説明にまとめた．

## 10-5 テスト駆動の実装

### `EXPLAIN`の構文

```rust
#[test]
fn explain_takes_a_select() {
    assert_eq!(
        statement("EXPLAIN SELECT * FROM t"),
        Ok(Statement::Explain(select_with("SELECT * FROM t")))
    );
    assert_eq!(
        statement("EXPLAIN VALUES (1)"),
        Err(ParseError::UnexpectedToken {
            token: Token::Keyword(Keyword::Values),
            position: 9
        })
    );
}
```

`statement`の`alt`に次の選択肢を加えた．

```rust
        preceded(literal(keyword(Keyword::Explain)), cut_err(select)).map(Statement::Explain),
```

### `SELECT`の名前解決を`plan::binder`に移す

```rust
#[test]
fn sort_key_prefers_an_output_name_to_a_table_column() {
    assert_eq!(
        order_by_sources("SELECT name AS id FROM users ORDER BY id"),
        Ok(vec![SortSource::Output(0)])
    );
}
```

`database`にあった`bind_select_items`，`column_name`，`sort_source`，`bind_filter`を`plan::binder`に移し，それらをまとめる`bind_select`を作った．
`SortOrder`も`exec::sort`から移した．`NULLS`の既定の位置を決めるのは，名前解決の仕事だからである．

```rust
pub fn bind_select(select: &Select, schema: &TableSchema) -> Result<BoundSelect, BindError> {
    let columns = &schema.columns;
    let (names, items) = bind_select_items(&select.items, columns)?;
    let filter = bind_filter(&select.filter, columns)?;
    let order_by = select
        .order_by
        .iter()
        .map(|key| {
            Ok(BoundOrderBy {
                source: sort_source(&key.expr, &names, &items, columns, select.distinct)?,
                order: SortOrder::new(key.descending, key.nulls_first),
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    Ok(BoundSelect {
        table: schema.name.clone(),
        table_columns: columns.iter().map(|column| column.name.clone()).collect(),
        names,
        items,
        filter,
        distinct: select.distinct,
        order_by,
        limit: select.limit,
    })
}
```

`limit: select.limit`は，`&Select`から`Limit`を取り出す．`Limit`に`Clone`と`Copy`を導出したので，ムーブせずに複製できる．
移したあと，`database`は`bind_select`の結果を使うように書き換え，引き継いだテストがすべて通ることを確かめた．

### 演算子

```rust
#[test]
fn filter_returns_rows_where_the_condition_is_true() {
    let predicate = BoundExpr::Binary {
        op: BinaryOp::Comparison(ComparisonOp::Gt),
        left: Box::new(BoundExpr::Column(0)),
        right: Box::new(BoundExpr::Constant(int(1))),
    };
    let input = scan(vec![vec![int(1)], vec![int(3)], vec![Value::Null], vec![int(2)]]);
    let mut filter = Filter::new(input, predicate);
    assert_eq!(
        collect_rows(&mut filter),
        Ok(vec![vec![int(3)], vec![int(2)]])
    );
}
```

`exec`にトレイト`Executor`を定義し，演算子を1つずつ作った．

```rust
pub trait Executor {
    fn next(&mut self) -> Result<Option<Row>, Error>;
}
```

```rust
pub struct Filter {
    input: Box<dyn Executor>,
    predicate: BoundExpr,
}

impl Executor for Filter {
    fn next(&mut self) -> Result<Option<Row>, Error> {
        while let Some(row) = self.input.next()? {
            if eval_condition(&self.predicate, &row, "WHERE")? {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }
}
```

`Filter`は，条件が真の行が見つかるまで子の`next`を呼ぶ．子が`None`を返したら，自分も`None`を返す．
`SeqScan`，`Project`，`Distinct`も同じ形である．`SeqScan`は`std::vec::IntoIter<Row>`を持ち，`next`で1行ずつ取り出す．

`Limit`は，最初に`offset`行を読み捨て，残りの行の数を数えながら返す．

```rust
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
```

`remaining`が0になったら，子の`next`を呼ばない．子の演算子は，それ以上の行を読まずに済む．

`Sort`は，最初の`next`で子の行をすべて読み，Iteration 9の`sort`で並べ替える．並べ替えた行は`Option<std::vec::IntoIter<Row>>`に持つ．

```rust
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
```

キーは，`SortKey::expr`を子の行について評価して求める．`KeyedRow`と`sort`は`Sort`の中だけで使うので，非公開にした．

### 実行計画

```rust
#[test]
fn sort_key_outside_the_select_list_is_a_hidden_column() {
    let plan = plan_sql("SELECT name FROM users ORDER BY id");
    assert_eq!(plan.columns(), ["NAME"]);
    let PlanNode::Project { input, exprs, .. } = plan else {
        panic!("not a Project");
    };
    assert_eq!(exprs, vec![BoundExpr::Column(0)]);
    let PlanNode::Sort { input, keys } = *input else {
        panic!("not a Sort");
    };
    assert_eq!(keys[0].expr, BoundExpr::Column(1));
    assert_eq!(input.columns(), ["NAME", "ID"]);
}
```

`let ... else`で，入れ子の`PlanNode`を1段ずつ取り出して確かめる．`*input`は`Box<PlanNode>`の中身を取り出す．

```rust
pub fn plan(select: &BoundSelect) -> PlanNode {
    let mut node = PlanNode::SeqScan {
        table: select.table.clone(),
        columns: select.table_columns.clone(),
    };
    if let Some(predicate) = &select.filter {
        node = PlanNode::Filter {
            input: Box::new(node),
            predicate: predicate.clone(),
        };
    }
    let mut exprs = select.items.clone();
    let mut names = select.names.clone();
    let mut keys = Vec::new();
    for key in &select.order_by {
        let index = match &key.source {
            SortSource::Output(index) => *index,
            SortSource::Input(expr) => {
                names.push(expr.display(&select.table_columns));
                exprs.push(expr.clone());
                exprs.len() - 1
            }
        };
        keys.push(SortKey {
            expr: BoundExpr::Column(index),
            order: key.order,
        });
    }
    let hidden_columns = exprs.len() > select.items.len();
    node = PlanNode::Project {
        input: Box::new(node),
        exprs,
        names,
    };
    // Distinct，Sort，Limit も，その句があれば同じ形で node の上に重ねる
    if hidden_columns {
        node = PlanNode::Project {
            input: Box::new(node),
            exprs: (0..select.items.len()).map(BoundExpr::Column).collect(),
            names: select.names.clone(),
        };
    }
    node
}
```

`node`を下から順に`Box::new(node)`で包み，新しい親の演算子にする．
隠れた列の名前には，式の表示(`ID`)を使った．`EXPLAIN`の`Sort [ID]`は，この名前で表示される．

### `EXPLAIN`

```rust
#[test]
fn each_operator_is_a_line_indented_under_its_parent() {
    assert_eq!(
        explain(&plan_sql("SELECT name FROM users WHERE id > 1 ORDER BY name")),
        vec![
            "Sort [NAME]",
            "  Project [NAME]",
            "    Filter (ID > 1)",
            "      SeqScan USERS",
        ]
    );
}
```

```rust
pub fn explain(plan: &PlanNode) -> Vec<String> {
    let mut lines = Vec::new();
    explain_node(plan, 0, &mut lines);
    lines
}

fn explain_node(node: &PlanNode, depth: usize, lines: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    lines.push(format!("{indent}{}", label(node)));
    match node {
        PlanNode::SeqScan { .. } => {}
        PlanNode::Filter { input, .. }
        | PlanNode::Project { input, .. }
        | PlanNode::Distinct { input }
        | PlanNode::Sort { input, .. }
        | PlanNode::Limit { input, .. } => explain_node(input, depth + 1, lines),
    }
}
```

`label`は，演算子の式を子の演算子の列名で表示する(`predicate.display(input.columns())`)．
式の表示`BoundExpr::display`は`plan::binder`に置いた．`plan::planner`も隠れた列の名前に使うからである．

### `Database`をつなぐ

```rust
    fn select(&self, select: &Select) -> Result<StatementResult, Error> {
        let plan = self.plan(select)?;
        let columns = plan.columns().to_vec();
        let mut executor = build(&plan, &self.rows);
        let rows = collect_rows(executor.as_mut())?;
        Ok(StatementResult::Rows(QueryResult { columns, rows }))
    }

    fn explain(&self, select: &Select) -> Result<StatementResult, Error> {
        let plan = self.plan(select)?;
        let rows = explain(&plan)
            .into_iter()
            .map(|line| vec![Value::Varchar(line)])
            .collect();
        Ok(StatementResult::Rows(QueryResult {
            columns: vec![QUERY_PLAN_COLUMN.to_string()],
            rows,
        }))
    }
```

`build`は，`PlanNode`の各演算子に対応する`Executor`を作り，`Box<dyn Executor>`として返す．

```rust
pub fn build(plan: &PlanNode, tables: &HashMap<String, Vec<Row>>) -> Box<dyn Executor> {
    match plan {
        PlanNode::SeqScan { table, .. } => Box::new(SeqScan::new(tables[table].clone())),
        PlanNode::Filter { input, predicate } => {
            Box::new(Filter::new(build(input, tables), predicate.clone()))
        }
        // Project，Distinct，Sort，Limit も，子を build してから自分を作る
    }
}
```

`database`の`select`，`bind_order_by`，`remove_duplicates`などは不要になって消えた．`explain.rs`の残りの項目と`repl.rs`の項目は，ここまでの実装で通る．

## 10-6 振り返り

1. 演算子ごと，名前解決，実行計画，`EXPLAIN`のそれぞれに項目があるかを比べる．
2. `Filter<E: Executor>`のようにジェネリクスで持つと，`Filter<Project<SeqScan>>`のように，木の形が型に現れる．SQLの文によって木の形が変わるので，`build`の戻り値の型を1つに書けない．`Box<dyn Executor>`なら，どの形の木も同じ型になる．その代わり，`next`の呼び出しは実行時にvtableを引く．
3. `Executor`の木だけで`EXPLAIN`も表示するなら，トレイトに表示のメソッドを加える．演算子を加えるときは，その型に`next`と表示を実装すればよい．`PlanNode`を分けた設計では，`PlanNode`，`plan`，`explain`，`build`，演算子の型の5か所を変える．その代わり，実行計画を作る段階と実行する段階が分かれ，Iteration 17のように計画を選び直す処理を`PlanNode`の上で書ける．`EXPLAIN`も演算子を作らずに表示できる．
4. `Sort`は子の行をすべて読むので，`FETCH FIRST 1 ROWS ONLY`でも表の全行を読む．`ORDER BY`がなければ，`Limit`は1行を返したあと子の`next`を呼ばないので，`SeqScan`は1行しか返さない．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 10-7 発展課題

解答例である．`PlanNode::TopN { input, keys, count }`を加え，`plan`で`Sort`の代わりに置く．

```rust
    match select.limit.fetch {
        Some(fetch) if !keys.is_empty() => {
            node = PlanNode::TopN {
                input: Box::new(node),
                keys,
                count: select.limit.offset + fetch,
            };
        }
        _ if !keys.is_empty() => {
            node = PlanNode::Sort {
                input: Box::new(node),
                keys,
            };
        }
        _ => {}
    }
```

演算子`TopN`は`exec::sort`に置き，`Sort`と同じ`KeyedRow`と`sort`を使う．
読んでいる途中で，残す行が`count`の2倍を超えたら，並べ替えて`count`行に切り詰める．手元に置く行は`count`の2倍程度で済む．

```rust
    fn read_and_sort(&mut self) -> Result<Vec<Row>, Error> {
        let orders: Vec<SortOrder> = self.keys.iter().map(|key| key.order).collect();
        let mut rows = Vec::new();
        while let Some(row) = self.input.next()? {
            let keys = self
                .keys
                .iter()
                .map(|key| eval(&key.expr, &row))
                .collect::<Result<Row, _>>()?;
            rows.push(KeyedRow { values: row, keys });
            if rows.len() > self.count * 2 {
                sort(&mut rows, &orders);
                rows.truncate(self.count);
            }
        }
        sort(&mut rows, &orders);
        rows.truncate(self.count);
        Ok(rows.into_iter().map(|row| row.values).collect())
    }
```

`explain`の`label`には`TopN [キー] 行の数`を，`build`には`TopN::new`を加える．`PlanNode`の`match`はすべての列挙子を扱わなければならないので，`columns`と`explain_node`にも`TopN`の腕を加える．

```rust
#[test]
fn sort_under_fetch_first_becomes_top_n() {
    let mut db = database();
    assert_eq!(
        plan_lines(
            &mut db,
            "EXPLAIN SELECT name FROM users ORDER BY name DESC OFFSET 1 ROWS FETCH FIRST 1 ROWS ONLY"
        ),
        vec![
            "Limit OFFSET 1 FETCH FIRST 1",
            "  TopN [NAME DESC] 2",
            "    Project [NAME]",
            "      SeqScan USERS",
        ]
    );
    for n in 4..=20 {
        db.execute(&format!("INSERT INTO users VALUES ({n}, 'user{n:02}')"))
            .unwrap();
    }
    match db
        .execute("SELECT id FROM users ORDER BY id DESC OFFSET 1 ROWS FETCH FIRST 3 ROWS ONLY")
        .unwrap()
    {
        StatementResult::Rows(result) => assert_eq!(
            result.rows,
            vec![
                vec![Value::Integer(19)],
                vec![Value::Integer(18)],
                vec![Value::Integer(17)],
            ]
        ),
        other => panic!("not a query: {other:?}"),
    }
}
```

- 2つ目の問い合わせは，20行から4行を残すので，途中で何度も切り詰める．切り詰めても結果が`Sort`と同じことを確かめる．
- 既存の`hidden_sort_column_is_removed_by_the_top_project`の期待値は，`Sort [ID DESC]`が`TopN [ID DESC] 2`に変わる．
