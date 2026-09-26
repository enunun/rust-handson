# Iteration 9：並べ替えと件数の制限(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 9-1 準備

引き継いだ214個のテストがすべて通れば準備は終わりである．

## 9-2 文法と概念

課題の解答例である．

```rust
use std::cmp::Ordering;

#[derive(Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Ord for Point {
    fn cmp(&self, other: &Point) -> Ordering {
        self.y.cmp(&other.y).then_with(|| self.x.cmp(&other.x))
    }
}

impl PartialOrd for Point {
    fn partial_cmp(&self, other: &Point) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn rank(scores: &mut [(&str, u32)]) {
    scores.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
}

pub fn sort_none_last(values: &mut [Option<i32>]) {
    values.sort_by(|a, b| match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => a.cmp(b),
    });
}

pub fn page(items: &[i32], offset: usize, fetch: Option<usize>) -> Vec<i32> {
    items
        .iter()
        .copied()
        .skip(offset)
        .take(fetch.unwrap_or(usize::MAX))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let mut points = vec![
            Point { x: 2, y: 1 },
            Point { x: 1, y: 1 },
            Point { x: 0, y: 0 },
        ];
        points.sort();
        assert_eq!(
            points,
            vec![
                Point { x: 0, y: 0 },
                Point { x: 1, y: 1 },
                Point { x: 2, y: 1 }
            ]
        );

        let mut scores = vec![("bob", 70), ("alice", 90), ("carol", 70)];
        rank(&mut scores);
        assert_eq!(scores, vec![("alice", 90), ("bob", 70), ("carol", 70)]);

        let mut values = vec![Some(3), None, Some(1)];
        sort_none_last(&mut values);
        assert_eq!(values, vec![Some(1), Some(3), None]);

        assert_eq!(page(&[1, 2, 3, 4, 5], 1, Some(2)), vec![2, 3]);
        assert_eq!(page(&[1, 2, 3], 2, None), vec![3]);
    }
}
```

- `rank`は，点数を`b.1.cmp(&a.1)`と逆の順に比べて降順にする．`.reverse()`を付けても同じである．
- `Option<i32>`は`Ord`を導出していて，`None`は`Some`より小さい．`None`を最後にするには，比べ方を自分で書く．`Value`の`NULL`も同じ理由で，順序を自分で決める．

## 9-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 構文解析，値の順序，並べ替え，結合テストの順に並べた．
- 既存テストへの影響は，構文解析のテストの`Select`の期待値である．3つの項目に，`distinct: false`，`order_by: vec![]`，`limit: Limit::default()`を加えた．
- 並べ替えの単体テストは，`NULL`の既定の位置，`NULLS`による変更，2つ目のキー，安定性の4つに分けた．
- 結合テストの表は，`age`が`30`の行を2つと，`NULL`の行を1つ含む4行にした．キーが等しい行と`NULL`の行の両方を確かめられる．

## 9-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | `exec::sort`と，`database`からの依存，`exec::sort`から`value`への依存を加えた | 並べ替えを`database`から分けた |
| `code-types.md` | `OrderBy`，`Limit`，`SortOrder`，`KeyedRow`を加え，`Select`，`BindError`，`SqlState`を更新した．`Value`の順序を説明に書いた | 並べ替えと件数の制限を表す型ができた |
| `code-sequence.md` | `SELECT`の図に，`DISTINCT`，並べ替え，`OFFSET`と`FETCH FIRST`を加えた | 処理の順序が結果を決める |

- 図の`WHERE`の判定を，Iteration 8から使っている`matches_filter`に合わせた．
- `SortKey`と`SortSource`は`database`の中だけで使う型なので，型の図には描かなかった．キーの値の求め方は，処理の流れの図の説明に書いた．

## 9-5 テスト駆動の実装

### 構文

```rust
#[test]
fn order_by_with_directions_and_nulls() {
    assert_eq!(
        select_with("SELECT * FROM t ORDER BY a DESC, b + 1 ASC NULLS FIRST, c NULLS LAST")
            .order_by,
        vec![
            order_by(column("A"), true, None),
            order_by(
                arithmetic(ArithmeticOp::Add, column("B"), int(1)),
                false,
                Some(true)
            ),
            order_by(column("C"), false, Some(false)),
        ]
    );
}
```

キーワードと`OrderBy`，`Limit`を加え，`Select`に`distinct`，`order_by`，`limit`を加えた．
既存の`Select`の期待値にも新しいフィールドを加えた．`Limit`は`Default`を導出し，`Limit::default()`で書けるようにした．
テストの補助関数`select_with`は，文を構文解析して`Select`を取り出す．期待値に構文木全体を書かずに，確かめたいフィールドだけを比べられる．

```rust
fn select(input: &mut Tokens<'_>) -> ModalResult<Select> {
    preceded(
        literal(keyword(Keyword::Select)),
        cut_err((
            opt(literal(keyword(Keyword::Distinct))).map(|distinct| distinct.is_some()),
            separated(1.., select_item, literal(Token::Comma)),
            preceded(literal(keyword(Keyword::From)), identifier),
            opt(preceded(literal(keyword(Keyword::Where)), expr)),
            order_by,
            limit,
        )),
    )
    .map(|(distinct, items, from, filter, order_by, limit)| Select {
        distinct,
        items,
        from,
        filter,
        order_by,
        limit,
    })
    .parse_next(input)
}

fn order_by(input: &mut Tokens<'_>) -> ModalResult<Vec<OrderBy>> {
    opt(preceded(
        (
            literal(keyword(Keyword::Order)),
            literal(keyword(Keyword::By)),
        ),
        cut_err(separated(1.., order_by_item, literal(Token::Comma))),
    ))
    .map(|keys| keys.unwrap_or_default())
    .parse_next(input)
}

fn order_by_item(input: &mut Tokens<'_>) -> ModalResult<OrderBy> {
    (
        expr,
        opt(alt((
            literal(keyword(Keyword::Asc)).value(false),
            literal(keyword(Keyword::Desc)).value(true),
        ))),
        opt(preceded(
            literal(keyword(Keyword::Nulls)),
            alt((
                literal(keyword(Keyword::First)).value(true),
                literal(keyword(Keyword::Last)).value(false),
            )),
        )),
    )
        .map(|(expr, descending, nulls_first)| OrderBy {
            expr,
            descending: descending.unwrap_or(false),
            nulls_first,
        })
        .parse_next(input)
}
```

`order_by`は，`ORDER BY`がなければ空の並びを返す．`select`は，省略されたかどうかを区別せずに`Select`を組み立てられる．
選択項目が6つに増えたので，`to_select`をやめ，`map`のクロージャで`Select`を作った．

```rust
#[test]
fn row_count_must_be_an_integer() {
    assert_eq!(
        statement("SELECT * FROM t OFFSET a ROWS"),
        Err(ParseError::UnexpectedToken {
            token: Token::Identifier("A".to_string()),
            position: 24
        })
    );
}
```

```rust
fn limit(input: &mut Tokens<'_>) -> ModalResult<Limit> {
    (
        opt(preceded(
            literal(keyword(Keyword::Offset)),
            cut_err(terminated(row_count, literal(keyword(Keyword::Rows)))),
        )),
        opt(preceded(
            literal(keyword(Keyword::Fetch)),
            cut_err(delimited(
                literal(keyword(Keyword::First)),
                row_count,
                (
                    literal(keyword(Keyword::Rows)),
                    literal(keyword(Keyword::Only)),
                ),
            )),
        )),
    )
        .map(|(offset, fetch)| Limit {
            offset: offset.unwrap_or(0),
            fetch,
        })
        .parse_next(input)
}

fn row_count(input: &mut Tokens<'_>) -> ModalResult<usize> {
    any.verify_map(|token: &Spanned<Token>| match token.value {
        Token::Integer(n) => usize::try_from(n).ok(),
        _ => None,
    })
    .parse_next(input)
}
```

最初は`cut_err`を付けずに書いたところ，このテストは`OFFSET`の位置(17文字目)の構文エラーになった．
`opt`が`OFFSET a ROWS`全体を読めずに戻り，文の終わりを期待したところで`OFFSET`を読めなかったからである．
キーワードのあとを`cut_err`で囲むと，読めなかった`a`の位置でエラーになる．

`verify_map`に渡すクロージャは，引数の型を`&Spanned<Token>`と書いた．書かないと，次のエラーになる．

```text
error[E0282]: type annotations needed
   --> src/sql/parser.rs:316:21
    |
316 |     any.verify_map(|token| match token.value {
    |                     ^^^^^        ----- type must be known at this point
```

`match token.value`でフィールドを読む時点で，`token`の型が決まっていなければならない．

### 値の順序

```rust
#[test]
fn integer_and_bigint_are_compared_as_numbers() {
    assert!(Value::Integer(2) < Value::BigInt(10));
    assert!(Value::BigInt(-1) < Value::Integer(0));
    assert!(Value::Integer(1) < Value::BigInt(1));
}
```

```rust
impl Ord for Value {
    fn cmp(&self, other: &Value) -> Ordering {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
            (Value::BigInt(a), Value::BigInt(b)) => a.cmp(b),
            (Value::Integer(a), Value::BigInt(b)) => i64::from(*a).cmp(b).then(Ordering::Less),
            (Value::BigInt(a), Value::Integer(b)) => a.cmp(&i64::from(*b)).then(Ordering::Greater),
            (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
            (Value::Varchar(a), Value::Varchar(b)) => a.cmp(b),
            _ => self.type_rank().cmp(&other.type_rank()),
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Value {
    fn type_rank(&self) -> u8 {
        match self {
            Value::Integer(_) | Value::BigInt(_) => 0,
            Value::Boolean(_) => 1,
            Value::Varchar(_) => 2,
            Value::Null => 3,
        }
    }
}
```

- `Value`は`Eq`を導出しているので，`Integer(1) == BigInt(1)`は偽である．`cmp`もこれに合わせ，数が等しければ`then(Ordering::Less)`で`INTEGER`を先にした．
- 最後の腕は，`NULL`を含む組と，型の違う組を扱う．`NULL`の順番をいちばん大きくしたので，`NULL`はどの値よりも大きい．

### 並べ替え

```rust
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
```

`src/exec/sort.rs`を作った．最初の項目は，`a.cmp(b)`を向きに従って`reverse`するだけで通る．`NULL`は`Value`の順序で最大なので，降順では最初に来る．
`NULLS FIRST`と`NULLS LAST`の項目で，`NULL`の位置を向きと別に決める形にした．

```rust
impl SortOrder {
    pub fn new(descending: bool, nulls_first: Option<bool>) -> SortOrder {
        SortOrder {
            descending,
            nulls_first: nulls_first.unwrap_or(descending),
        }
    }
}

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
```

「`NULL`は最大」という既定は，`nulls_first`の既定値を`descending`と同じにすることで表した．降順なら最初，昇順なら最後である．

2つ目のキーの項目で，キーを先頭から順に比べる関数を加えた．

```rust
pub fn sort(rows: &mut [KeyedRow], orders: &[SortOrder]) {
    rows.sort_by(|a, b| compare_keys(&a.keys, &b.keys, orders));
}

fn compare_keys(a: &[Value], b: &[Value], orders: &[SortOrder]) -> Ordering {
    let mut ordering = Ordering::Equal;
    for ((a, b), order) in a.iter().zip(b).zip(orders) {
        ordering = ordering.then_with(|| compare(a, b, order));
    }
    ordering
}
```

`then_with`は，それまでの結果が`Equal`のときだけクロージャを呼ぶ．前のキーで順序が決まった行は，後のキーを比べない．
安定性の項目は，`sort_by`が安定なので，そのまま通る．

### `SELECT`の実行

```rust
#[test]
fn orders_by_a_column_descending_and_limits_the_rows() {
    let mut db = database();
    assert_eq!(
        rows(
            &mut db,
            "SELECT name FROM users ORDER BY name DESC OFFSET 1 ROWS FETCH FIRST 2 ROWS ONLY"
        ),
        vec![vec![varchar("carol")], vec![varchar("bob")]]
    );
}
```

行ごとに，結果の値と並べ替えのキーの値の組`KeyedRow`を作る．並べ替えたあとで，`skip`と`take`で行を取り出し，結果の値だけを残す．

```rust
        let mut rows = Vec::new();
        for row in &self.rows[&select.from] {
            if !matches_filter(&filter, row)? {
                continue;
            }
            let values = exprs
                .iter()
                .map(|expr| eval(expr, row))
                .collect::<Result<Vec<_>, _>>()?;
            let keys = sort_keys
                .iter()
                .map(|key| match &key.source {
                    SortSource::Output(index) => Ok(values[*index].clone()),
                    SortSource::Input(expr) => eval(expr, row),
                })
                .collect::<Result<Vec<_>, _>>()?;
            rows.push(KeyedRow { values, keys });
        }
        if select.distinct {
            remove_duplicates(&mut rows);
        }
        let orders: Vec<SortOrder> = sort_keys.iter().map(|key| key.order).collect();
        sort(&mut rows, &orders);
        let rows = rows
            .into_iter()
            .skip(select.limit.offset)
            .take(select.limit.fetch.unwrap_or(usize::MAX))
            .map(|row| row.values)
            .collect();
```

最初の項目を通した時点では，キーの式を表の列で`bind`して評価していた(`SortSource::Input`)．
別名の項目を通すため，名前だけのキーは結果の列名から先に探す．

```rust
enum SortSource {
    /// 結果の列の値を使う．
    Output(usize),
    /// 表の行について式を評価する．
    Input(BoundExpr),
}

fn sort_source(
    expr: &Expr,
    names: &[String],
    exprs: &[BoundExpr],
    columns: &[Column],
    distinct: bool,
) -> Result<SortSource, Error> {
    let output = match expr {
        Expr::Column(name) => names.iter().position(|output| output == name),
        _ => None,
    };
    if let Some(index) = output {
        return Ok(SortSource::Output(index));
    }
    let bound = bind(expr, columns)?;
    if let Some(index) = exprs.iter().position(|output| *output == bound) {
        return Ok(SortSource::Output(index));
    }
    if distinct {
        return Err(BindError::OrderByNotInSelectList.into());
    }
    Ok(SortSource::Input(bound))
}
```

- `if let Expr::Column(name) = expr`の中に`if let Some(index)`を入れ子にすると，`cargo clippy`がまとめるよう求める．`match`で`Option<usize>`を求めてから`if let`で調べる形にした．
- 選択項目と同じ式のキー(`SELECT DISTINCT age + 1 ... ORDER BY age + 1`)も，結果の列の値を使う．`BoundExpr`は`PartialEq`を導出しているので，`==`で比べられる．
- `DISTINCT`の項目は，この時点で`OrderByNotInSelectList`を返すようにした．`error`に`42P10`への変換を加えた．

`DISTINCT`は，`retain`と`contains`で結果の値が重なる行を除く．

```rust
fn remove_duplicates(rows: &mut Vec<KeyedRow>) {
    let mut seen: Vec<Row> = Vec::new();
    rows.retain(|row| {
        if seen.contains(&row.values) {
            false
        } else {
            seen.push(row.values.clone());
            true
        }
    });
}
```

`Value::Null == Value::Null`は真なので(導出した`PartialEq`)，`NULL`の行も1つにまとまる．SQLの`=`とは違う規則だが，重複の判定の規則とは一致する．
`order_by.rs`の残りの項目は，ここまでの実装で通る．

## 9-6 振り返り

1. 構文，値の順序，並べ替え，`SELECT`の組み合わせのそれぞれに項目があるかを比べる．
2. `sort_by`に渡す比較は，すべての組の順序を矛盾なく決めなければならない．式の結果の列には`INTEGER`と`BIGINT`が混ざりうる．どんな型の組でも比べられるように，全順序(`Ord`)にした．`PartialOrd`だけにすると，比べられない組で`None`が返る．その扱いを，並べ替えのたびに決める必要がある．
3. 結果の値だけを並べ替えると，選択していない列の値は結果の行に残っていないので，`ORDER BY id`を扱えない．キーの値を結果の値と別に持てば，キーを求める段階で表の行を読める．
4. `HashSet<Row>`にすると，行ごとの検査が1回の探索で済み，行が多いときに速くなる．`Value`は`Hash`を導出しているので，`Row`もそのまま入る．`contains`は短く書けるが，比べる回数は行の数の2乗に比例する．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 9-7 発展課題

解答例である．`Limit`に`with_ties: bool`を加え，キーワード`WITH`と`TIES`を加える．

```rust
fn limit(input: &mut Tokens<'_>) -> ModalResult<Limit> {
    (
        opt(preceded(
            literal(keyword(Keyword::Offset)),
            cut_err(terminated(row_count, literal(keyword(Keyword::Rows)))),
        )),
        opt(preceded(
            literal(keyword(Keyword::Fetch)),
            cut_err(preceded(
                literal(keyword(Keyword::First)),
                (
                    row_count,
                    preceded(literal(keyword(Keyword::Rows)), fetch_end),
                ),
            )),
        )),
    )
        .map(|(offset, fetch)| match fetch {
            Some((count, with_ties)) => Limit {
                offset: offset.unwrap_or(0),
                fetch: Some(count),
                with_ties,
            },
            None => Limit {
                offset: offset.unwrap_or(0),
                fetch: None,
                with_ties: false,
            },
        })
        .parse_next(input)
}

/// `FETCH FIRST n ROWS`のあとの`ONLY`か`WITH TIES`を読み，`WITH TIES`なら`true`を返す．
fn fetch_end(input: &mut Tokens<'_>) -> ModalResult<bool> {
    alt((
        literal(keyword(Keyword::Only)).value(false),
        (
            literal(keyword(Keyword::With)),
            literal(keyword(Keyword::Ties)),
        )
            .value(true),
    ))
    .parse_next(input)
}
```

`Database::select`では，`take`の代わりに，残す行の数`end`を求めて`truncate`する．`WITH TIES`なら，次の行のキーが`end`行目と等しいあいだ，`end`を増やす．
`exec::sort`の`compare_keys`を`pub`にして使う．

```rust
        let mut rows: Vec<KeyedRow> = rows.into_iter().skip(select.limit.offset).collect();
        if let Some(fetch) = select.limit.fetch {
            let mut end = fetch.min(rows.len());
            if select.limit.with_ties && end > 0 {
                while end < rows.len()
                    && compare_keys(&rows[end - 1].keys, &rows[end].keys, &orders)
                        == Ordering::Equal
                {
                    end += 1;
                }
            }
            rows.truncate(end);
        }
        let rows = rows.into_iter().map(|row| row.values).collect();
```

```rust
#[test]
fn fetch_with_ties_returns_rows_equal_to_the_last_one() {
    let mut db = database();
    assert_eq!(
        ids(
            &mut db,
            "SELECT id FROM users ORDER BY age DESC NULLS LAST FETCH FIRST 1 ROWS WITH TIES"
        ),
        vec![1, 4]
    );
    assert_eq!(
        ids(
            &mut db,
            "SELECT id FROM users ORDER BY age OFFSET 1 ROWS FETCH FIRST 1 ROWS WITH TIES"
        ),
        vec![1, 4]
    );
}
```

- `min`は2つの値の小さいほうを返す．`FETCH FIRST`の行の数が残りの行より多くても，`end`は行の数を超えない．
- `truncate(n)`は，`Vec`の先頭の`n`個を残して後ろを捨てる．
- 既存の構文解析のテストの`Limit`の期待値に`with_ties: false`を加える．
