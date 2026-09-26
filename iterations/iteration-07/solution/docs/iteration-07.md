# Iteration 7：WHERE，列の選択，別名(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 7-1 準備

引き継いだ146個のテストがすべて通れば準備は終わりである．

## 7-2 文法と概念

課題の解答例である．

```rust
pub fn odd_squares(numbers: &[i64]) -> Vec<i64> {
    numbers.iter().filter(|&&n| n % 2 != 0).map(|n| n * n).collect()
}

pub fn count_above(numbers: &[i64], threshold: i64) -> usize {
    numbers.iter().filter(|&&n| n > threshold).count()
}

pub fn lengths(words: &[&str]) -> Result<Vec<usize>, String> {
    words
        .iter()
        .map(|w| {
            if w.is_empty() {
                Err("empty word".to_string())
            } else {
                Ok(w.chars().count())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        assert_eq!(odd_squares(&[1, 2, 3, 4, 5]), vec![1, 9, 25]);
        let names = vec!["alice", "bob", "carol"];
        assert_eq!(names.iter().position(|&name| name == "bob"), Some(1));
        assert_eq!(count_above(&[1, 5, 10], 4), 2);
        assert_eq!(lengths(&["ab", "日本"]), Ok(vec![2, 2]));
        assert_eq!(lengths(&["ab", ""]), Err("empty word".to_string()));
    }
}
```

- `filter`のクロージャは，要素への参照を受け取る．`iter()`の要素がすでに`&i64`なので，`filter`には`&&i64`が渡る．`|&&n|`で2つの参照を外している．
- `count`は，イテレーターの要素の数を返す．
- `lengths`のクロージャは，`Ok`と`Err`を返す分岐を持つ．本体が複数の文なので波括弧で囲む．

## 7-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 構文解析，名前解決，評価，結合テストの順に並べた．名前解決は，リテラル，列，式の中の列，ない列の順にした．
- 評価には，列の値を行から取る項目と，条件の評価の項目を加えた．条件の評価は，真，偽，`NULL`，真偽値でない値の4通りを確かめる．
- 結合テストでは，`age`が`NULL`の行を含む3行の表を用意した．`WHERE age > 26`と`WHERE NOT age > 26`の両方で`NULL`の行が残らないことを確かめる．

## 7-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | `plan::binder`と，`database`，`exec::eval`，`error`からの依存を加えた | 名前解決の段階ができた．評価は`BoundExpr`を読む |
| `code-types.md` | `BoundExpr`，`SelectItem`，`BindError`，`EvalError::ArgumentNotBoolean`，`Expr::Column`を加え，`Select`を更新した | `Expr`と`BoundExpr`を別の型にした |
| `code-sequence.md` | `INSERT`の図を`SELECT`の図に替え，`INSERT`の説明は図の下に残した | 名前解決と行ごとの評価が，このIterationの中心である |

- `exec::eval`は`sql::ast`の演算子の型(`UnaryOp`，`BinaryOp`など)を読むが，`Expr`は読まない．`BoundExpr`が演算子の型をそのまま使うからである．

## 7-5 テスト駆動の実装

### 列の名前と選択項目の構文

```rust
#[test]
fn select_expressions_with_and_without_alias() {
    assert_eq!(
        statement("SELECT a, b + 1 AS c, d e FROM t"),
        Ok(Statement::Select(Select {
            items: vec![
                SelectItem::Expr {
                    expr: column("A"),
                    alias: None
                },
                SelectItem::Expr {
                    expr: arithmetic(ArithmeticOp::Add, column("B"), int(1)),
                    alias: Some("C".to_string())
                },
                SelectItem::Expr {
                    expr: column("D"),
                    alias: Some("E".to_string())
                },
            ],
            from: "T".to_string(),
            filter: None
        }))
    );
}
```

`Expr::Column`，`SelectItem`を加え，`Select`を`items`，`from`，`filter`にした．列の名前は，`operand`で識別子を読んで作る．

```rust
fn operand(input: &mut Tokens<'_>) -> ModalResult<Expr> {
    alt((
        any.verify_map(constant),
        identifier.map(Expr::Column),
        preceded(
            literal(Token::LParen),
            cut_err(terminated(expr, literal(Token::RParen))),
        ),
    ))
    .parse_next(input)
}
```

```rust
fn select(input: &mut Tokens<'_>) -> ModalResult<Select> {
    preceded(
        literal(keyword(Keyword::Select)),
        cut_err((
            separated(1.., select_item, literal(Token::Comma)),
            preceded(literal(keyword(Keyword::From)), identifier),
            opt(preceded(literal(keyword(Keyword::Where)), expr)),
        )),
    )
    .map(to_select)
    .parse_next(input)
}

fn select_item(input: &mut Tokens<'_>) -> ModalResult<SelectItem> {
    alt((
        literal(Token::Star).map(|_| SelectItem::Wildcard),
        (
            expr,
            opt(preceded(opt(literal(keyword(Keyword::As))), identifier)),
        )
            .map(to_select_expr),
    ))
    .parse_next(input)
}
```

`*`の選択肢を`.value(SelectItem::Wildcard)`と書くと，`SelectItem`が`Clone`でないのでコンパイルエラーになる．
クロージャ`|_| SelectItem::Wildcard`は，読んだトークンを捨てて新しい値を作るので，`Clone`は要らない．
既存の`SELECT * FROM t`の項目は，期待値を新しいフィールドに書き換えた．

### 名前解決

```rust
#[test]
fn column_name_becomes_its_index() {
    assert_eq!(bind(&expr("name"), &columns()), Ok(BoundExpr::Column(1)));
}
```

`src/plan.rs`と`src/plan/binder.rs`を作り，`BoundExpr`と`bind`を定義した．
`bind`は`Expr`を再帰的にたどる．リテラルは値に，列の名前は番号にする．

```rust
pub fn bind(expr: &Expr, columns: &[Column]) -> Result<BoundExpr, BindError> {
    let bound = match expr {
        Expr::Integer(n) => BoundExpr::Constant(match i32::try_from(*n) {
            Ok(n) => Value::Integer(n),
            Err(_) => Value::BigInt(*n),
        }),
        Expr::Boolean(b) => BoundExpr::Constant(Value::Boolean(*b)),
        Expr::String(s) => BoundExpr::Constant(Value::Varchar(s.clone())),
        Expr::Null => BoundExpr::Constant(Value::Null),
        Expr::Column(name) => {
            let index = columns
                .iter()
                .position(|column| column.name == *name)
                .ok_or_else(|| BindError::UndefinedColumn {
                    column: name.clone(),
                })?;
            BoundExpr::Column(index)
        }
        Expr::Unary { op, operand } => BoundExpr::Unary {
            op: op.clone(),
            operand: Box::new(bind(operand, columns)?),
        },
        // Binary と IsNull も同じ形で，子の式を bind する
    };
    Ok(bound)
}
```

`position`のクロージャは，外の変数`name`を捕捉している．`ok_or_else`は，見つからなかったときだけエラーの値を作る．
リテラルの項目，式の中の列の項目，ない列の項目は，この実装で通る．

### 評価を`BoundExpr`に移す

```rust
#[test]
fn column_takes_its_value_from_the_row() {
    let row = vec![Value::Integer(3), Value::Varchar("x".to_string())];
    assert_eq!(eval(&BoundExpr::Column(1), &row), Ok(Value::Varchar("x".to_string())));
}
```

`eval`が`BoundExpr`と行を受け取るように変えた．リテラルの変換は名前解決に移ったので，定数はそのまま複製する．

```rust
pub fn eval(expr: &BoundExpr, row: &[Value]) -> Result<Value, EvalError> {
    match expr {
        BoundExpr::Constant(value) => Ok(value.clone()),
        BoundExpr::Column(index) => Ok(row[*index].clone()),
        BoundExpr::Unary { op, operand } => {
            let value = eval(operand, row)?;
            eval_unary(op, value)
        }
        // Binary と IsNull も，子の式を同じ row で評価する
    }
}
```

既存の評価のテストは，補助関数で`bind`してから評価するように直した．

```rust
    fn eval_sql(expr: &str) -> Result<Value, EvalError> {
        match parse(&tokenize(&format!("VALUES ({expr})")).unwrap()).unwrap() {
            Statement::Values(values) => eval(&bind(&values.rows[0][0], &[]).unwrap(), &[]),
            other => panic!("not a VALUES statement: {other:?}"),
        }
    }
```

### 条件の評価

```rust
#[test]
fn condition_must_be_boolean() {
    assert_eq!(
        eval_condition(&BoundExpr::Constant(Value::Integer(1)), &[], "WHERE"),
        Err(EvalError::ArgumentNotBoolean {
            clause: "WHERE",
            found: "integer"
        })
    );
}
```

`eval_condition`は，条件の値を`bool`にする．`NULL`は偽と同じく`false`にする．
どの句の条件かをメッセージに入れるため，句の名前を引数で受け取る．

```rust
pub fn eval_condition(
    expr: &BoundExpr,
    row: &[Value],
    clause: &'static str,
) -> Result<bool, EvalError> {
    match eval(expr, row)? {
        Value::Boolean(b) => Ok(b),
        Value::Null => Ok(false),
        other => Err(EvalError::ArgumentNotBoolean {
            clause,
            found: other.type_name(),
        }),
    }
}
```

### `SELECT`の実行

```rust
#[test]
fn selects_columns_and_expressions_with_aliases() {
    let result = query(
        &mut database(),
        "SELECT name, id * 10 AS score FROM users WHERE id >= 2",
    );
    assert_eq!(result.columns, vec!["NAME", "SCORE"]);
    assert_eq!(
        result.rows,
        vec![
            vec![varchar("bob"), Value::Integer(20)],
            vec![varchar("carol"), Value::Integer(30)],
        ]
    );
}
```

選択項目と条件を先に名前解決してから，行ごとに条件を調べ，選択項目を評価する．

```rust
    fn select(&self, select: &Select) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&select.from)?;
        let (columns, exprs) = bind_select_items(&select.items, &schema.columns)?;
        let filter = match &select.filter {
            Some(filter) => Some(bind(filter, &schema.columns)?),
            None => None,
        };
        let mut rows = Vec::new();
        for row in &self.rows[&select.from] {
            if !matches_filter(&filter, row)? {
                continue;
            }
            let values = exprs
                .iter()
                .map(|expr| eval(expr, row))
                .collect::<Result<Vec<_>, _>>()?;
            rows.push(values);
        }
        Ok(StatementResult::Rows(QueryResult { columns, rows }))
    }
```

`if let Some(filter) = &filter`の中に条件の`if`を入れ子にすると，`cargo clippy`がまとめるよう求めた．
条件がない場合も含めて「行が条件を満たすか」を返す`matches_filter`に分けた．

```rust
fn matches_filter(filter: &Option<BoundExpr>, row: &[Value]) -> Result<bool, Error> {
    match filter {
        Some(filter) => Ok(eval_condition(filter, row, "WHERE")?),
        None => Ok(true),
    }
}
```

選択項目の名前解決では，`*`をすべての列に展開する．`extend`で，イテレーターの要素をまとめて加える．

```rust
fn bind_select_items(
    items: &[SelectItem],
    columns: &[Column],
) -> Result<(Vec<String>, Vec<BoundExpr>), Error> {
    let mut names = Vec::new();
    let mut exprs = Vec::new();
    for item in items {
        match item {
            SelectItem::Wildcard => {
                names.extend(columns.iter().map(|column| column.name.clone()));
                exprs.extend((0..columns.len()).map(BoundExpr::Column));
            }
            SelectItem::Expr { expr, alias } => {
                names.push(column_name(expr, alias));
                exprs.push(bind(expr, columns)?);
            }
        }
    }
    Ok((names, exprs))
}
```

`error`には，`BindError`を`42703`に，`ArgumentNotBoolean`を`42804`に変換する規則を加えた．
`select.rs`の残りの項目は，ここまでの実装で通る．

### Refactor：イテレーターで書き直す

`for`で`Vec`を組み立てていたところを，`map`と`collect`で書き直した．

```rust
fn target_columns(schema: &TableSchema, names: Option<Vec<String>>) -> Result<Vec<usize>, Error> {
    match names {
        Some(names) => Ok(names
            .iter()
            .map(|name| schema.column_index(name))
            .collect::<Result<Vec<_>, _>>()?),
        None => Ok((0..schema.columns.len()).collect()),
    }
}

fn evaluate_values(values: &Values) -> Result<QueryResult, Error> {
    let rows = values
        .rows
        .iter()
        .map(|exprs| exprs.iter().map(evaluate_constant).collect())
        .collect::<Result<Vec<Vec<Value>>, Error>>()?;
    let columns = (1..=values.rows[0].len())
        .map(|index| format!("COLUMN{index}"))
        .collect();
    Ok(QueryResult { columns, rows })
}
```

`evaluate_values`の内側の`collect`は，外側の`collect`の型から`Result<Vec<Value>, Error>`と推論される．

## 7-6 振り返り

1. 名前解決，評価，条件の評価，`SELECT`の組み合わせのそれぞれに項目があるかを比べる．
2. `Expr`に`ColumnIndex(usize)`を加えて同じ型のまま名前解決すると，評価の`match`は，名前の残った`Expr::Column(String)`も扱わなければならない．解決していない式を評価するのは誤りなので，`panic!`か，新しいエラーを返すことになる．型を分ければ，評価に渡せるのは解決済みの式だけになり，この誤りはコンパイル時に防げる．
3. `ferrodb`は条件の型を行ごとに調べるので，行のない表では`WHERE 1`もエラーにならず，0行を返す．名前解決で調べる設計(PostgreSQL)では，行の数と関係なくエラーである．名前解決で型を調べるには，式ごとに型を推論する規則が必要になる．
4. 行を絞り込むところを`filter`で書くと，条件の評価のエラーを返す方法が要る(`filter`のクロージャは`bool`を返す)．選択項目の評価を`for`で書くと，`Vec`を用意して`push`する分だけ長くなる．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 7-7 発展課題

解答例である．`IN`を後置演算子として読み，演算子を適用する関数の中で`(式, ...)`を読む．
winnowの`Postfix`に渡す関数は入力を受け取るので，演算子のあとのトークンを続けて読める．

```rust
const IN: i64 = 6;

            literal(keyword(Keyword::In)).value(Postfix(IN, in_list)),

/// `IN`のあとの`(式, ...)`を読み，`operand = 式1 OR operand = 式2 OR ...`にする．
fn in_list(input: &mut Tokens<'_>, operand: Expr) -> ModalResult<Expr> {
    let list: Vec<Expr> = cut_err(delimited(
        literal(Token::LParen),
        separated(1.., expr, literal(Token::Comma)),
        literal(Token::RParen),
    ))
    .parse_next(input)?;
    let condition = list
        .into_iter()
        .map(|item| comparison(ComparisonOp::Eq, operand.clone(), item))
        .reduce(|left, right| binary(BinaryOp::Or, left, right))
        .expect("the list has at least one expression");
    Ok(condition)
}
```

```rust
#[test]
fn in_list_follows_three_valued_logic() {
    let result = query(&mut database(), "SELECT id FROM users WHERE id IN (1, 3)");
    assert_eq!(result.rows, vec![vec![Value::Integer(1)], vec![Value::Integer(3)]]);
    let result = query(
        &mut database(),
        "SELECT id, age IN (30, NULL) FROM users",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Integer(1), Value::Boolean(true)],
            vec![Value::Integer(2), Value::Null],
            vec![Value::Integer(3), Value::Null],
        ]
    );
}
```

- `reduce`は，最初の要素から始めて，クロージャで要素を1つずつ畳み込む．要素がなければ`None`を返す．
- 左辺の式を比較ごとに使うので，`Expr`に`Clone`を導出し，`operand.clone()`で複製する．
- `OR`の3値論理がそのまま`IN`の規則になるので，評価は変わらない．
- キーワード`IN`も加える．
