# Iteration 11：結合(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 11-1 準備

引き継いだ262個のテストがすべて通れば準備は終わりである．

## 11-2 文法と概念

課題の解答例である．

```rust
pub struct Task {
    pub current: Option<String>,
}

impl Task {
    pub fn finish(&mut self) -> Option<String> {
        self.current.take()
    }
}

pub fn count_x(words: &[&str]) -> &'static str {
    let found: Vec<usize> = (0..words.len()).filter(|&i| words[i] == "x").collect();
    match found.as_slice() {
        [] => "none",
        [_] => "one",
        _ => "many",
    }
}

pub fn nest(words: &[&str]) -> String {
    let (first, rest) = words.split_first().expect("at least one word");
    rest.iter()
        .fold(first.to_string(), |left, right| format!("({left}+{right})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let mut task = Task {
            current: Some("a".to_string()),
        };
        assert_eq!(task.finish(), Some("a".to_string()));
        assert_eq!(task.current, None);
        assert_eq!(count_x(&["a", "b"]), "none");
        assert_eq!(count_x(&["x", "b"]), "one");
        assert_eq!(count_x(&["x", "x"]), "many");
        assert_eq!(nest(&["a", "b", "c"]), "((a+b)+c)");
    }
}
```

`take`を使わずに`self.current`を返すと，次のエラーになる．

```text
error[E0507]: cannot move out of `self.current` which is behind a mutable reference
 --> src/lib.rs:7:9
  |
7 |         self.current
  |         ^^^^^^^^^^^^ move occurs because `self.current` has type `Option<String>`, which does not implement the `Copy` trait
```

`&mut self`は借りているだけなので，フィールドの値をムーブして持ち去れない．`take`は`None`を残して中身を取り出すので，借りた構造体を壊さない．

- `[_]`は，要素が1つのスライスに一致し，要素は使わない．
- `split_first`は，スライスを最初の要素と残りに分け，`Option`で返す．空なら`None`である．

## 11-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 字句解析，構文解析，名前解決，実行計画，`EXPLAIN`，演算子，結合テストの順に並べた．
- 既存テストへの影響は，構文解析の`Select::from`，名前解決の`bind`と`bind_select`の引数，実行計画と`EXPLAIN`の列名である．
- 結合テストの表は，ロードマップの完成形の`emp`と`dept`にした．`dept`が`NULL`の社員(部署のない社員)も1人いる．この社員の行で，左外部結合の`NULL`を確かめられる．
- エラーは，あいまいな列，`FROM`にない表，表にない修飾した列，同じ名前の表，真偽値でない`ON`の5つを確かめる．

## 11-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | `exec::join`と，`exec::build`からの依存，`exec::join`から`exec`，`exec::eval`，`plan::binder`，`sql::ast`，`error`，`value`への依存を加えた．`plan::planner`と`plan::explain`から`sql::ast`への依存を加えた | 結合の演算子ができた．`JoinKind`を構文木から実行計画まで使う |
| `code-types.md` | `TableRef`，`Join`，`JoinKind`，`Scope`，`ScopeColumn`，`BoundFrom`，`NestedLoopJoin`を加え，`Select`，`Expr`，`BoundSelect`，`BindError`，`SqlState`，`PlanNode`を更新した | 複数の表を扱う型ができた |
| `code-sequence.md` | 入れ子ループ結合の図を加えた | 内側の行を繰り返し照らし合わせる流れを示す |

- `plan::binder`は，Iteration 10までは`TableSchema`を受け取っていた．`FROM`の表を自分で引くため，`Catalog`を受け取るようにした．
- `JoinKind`は構文木の型だが，名前解決，実行計画，演算子でもそのまま使う．種類ごとに別の型を作らずに済む．

## 11-5 テスト駆動の実装

### 修飾した列名と`FROM`の構文

```rust
#[test]
fn joins_are_left_associative() {
    assert_eq!(
        select_with("SELECT * FROM a JOIN b ON TRUE LEFT JOIN c ON FALSE").from,
        join(
            join(
                table("A"),
                table("B"),
                JoinKind::Inner,
                Some(Expr::Boolean(true))
            ),
            table("C"),
            JoinKind::Left,
            Some(Expr::Boolean(false))
        )
    );
}
```

字句解析に`.`を，構文木に`TableRef`，`Join`，`JoinKind`，`Expr::QualifiedColumn`を加えた．既存の`Select`の期待値の`from`は，`table("T")`に書き換えた．

```rust
fn from_clause(input: &mut Tokens<'_>) -> ModalResult<TableRef> {
    separated(1.., joined_table, literal(Token::Comma))
        .map(|tables: Vec<TableRef>| {
            tables
                .into_iter()
                .reduce(|left, right| join(left, right, JoinKind::Cross, None))
                .expect("FROM has at least one table")
        })
        .parse_next(input)
}

fn joined_table(input: &mut Tokens<'_>) -> ModalResult<TableRef> {
    (table_primary, repeat(0.., join_clause))
        .map(
            |(first, joins): (TableRef, Vec<(JoinKind, TableRef, Option<Expr>)>)| {
                joins
                    .into_iter()
                    .fold(first, |left, (kind, right, condition)| {
                        join(left, right, kind, condition)
                    })
            },
        )
        .parse_next(input)
}
```

- `joined_table`は，最初の表から始めて，`JOIN`を1つずつ左に畳み込む．`fold`の初期値が最初の表である．
- `from_clause`は，`,`で区切った並びを`CROSS JOIN`でつなぐ．`separated(1.., ...)`なので並びは空でなく，`reduce`は`Some`を返す．
- `map`のクロージャの引数には型を書いた．書かないと，`repeat`が集める先の型が決まらず，次のエラーになる．

```text
error[E0282]: type annotations needed for `(TableRef, _)`
   --> src/sql/parser.rs:270:14
    |
270 |             |(first, joins)| {
    |              ^^^^^^^^^^^^^^
271 |                 joins
    |                 ----- type must be known at this point
```

`join_clause`は，`CROSS JOIN 表`か，`join_kind`，表，`ON 条件`を読む．`join_kind`は`[INNER] JOIN`と`LEFT [OUTER] JOIN`を区別する．
`JOIN`のあとを`cut_err`で囲んだ．`FROM a JOIN b`は`ON`の前で文が終わるので，`UnexpectedEnd`になる．

### 名前解決と`Scope`

```rust
#[test]
fn unqualified_name_found_in_two_tables_is_ambiguous() {
    assert_eq!(
        bind(&expr("id"), &two_tables()),
        Err(BindError::AmbiguousColumn {
            column: "ID".to_string()
        })
    );
}
```

`bind`が受け取る列の並びを，`&[Column]`から`Scope`に変えた．既存のテストは，`Scope::table("USERS", &columns())`と`Scope::default()`を渡すように直した．

```rust
    fn resolve(&self, table: Option<&str>, name: &str) -> Result<usize, BindError> {
        match table {
            Some(table) if !self.has_table(table) => {
                return Err(BindError::MissingFromEntry {
                    table: table.to_string(),
                });
            }
            _ => {}
        }
        let found: Vec<usize> = (0..self.columns.len())
            .filter(|&index| self.columns[index].is(table, name))
            .collect();
        match (found.as_slice(), table) {
            ([index], _) => Ok(*index),
            ([], None) => Err(BindError::UndefinedColumn {
                column: name.to_string(),
            }),
            ([], Some(table)) => Err(BindError::UndefinedQualifiedColumn {
                table: table.to_string(),
                column: name.to_string(),
            }),
            (_, _) => Err(BindError::AmbiguousColumn {
                column: name.to_string(),
            }),
        }
    }
```

見つかった列の番号の並びと，修飾の有無の組で`match`する．
1つ目の項目(1つの表にだけある名前)は`([index], _)`の腕だけで通る．あいまいな名前と修飾した名前の項目で，残りの腕を加えた．

`bind_select`は，カタログから`FROM`の表を引き，`Scope`を作ってから選択項目を解決する．

```rust
fn bind_from(
    table_ref: &TableRef,
    catalog: &Catalog,
    tables: &mut Vec<String>,
) -> Result<(BoundFrom, Scope), BindError> {
    match table_ref {
        TableRef::Table { name, alias } => {
            let schema = catalog
                .table(name)
                .map_err(|_| BindError::UndefinedTable {
                    table: name.clone(),
                })?;
            let qualifier = alias.as_ref().unwrap_or(name);
            if tables.contains(qualifier) {
                return Err(BindError::DuplicateTableName {
                    table: qualifier.clone(),
                });
            }
            tables.push(qualifier.clone());
            let scope = Scope::table(qualifier, &schema.columns);
            let from = BoundFrom::Table {
                table: name.clone(),
                alias: alias.clone(),
                columns: scope.qualified_names(),
            };
            Ok((from, scope))
        }
        TableRef::Join(join) => {
            let (left, left_scope) = bind_from(&join.left, catalog, tables)?;
            let (right, right_scope) = bind_from(&join.right, catalog, tables)?;
            let scope = left_scope.join(right_scope);
            let condition = bind_filter(&join.condition, &scope)?;
            let from = BoundFrom::Join {
                left: Box::new(left),
                right: Box::new(right),
                kind: join.kind,
                condition,
            };
            Ok((from, scope))
        }
    }
}
```

- `Scope::join`は`self`を受け取り，右の列を加えた並びを返す．左右の`Scope`はもう使わないので，複製せずに済む．
- `tables`には，それまでに現れた表の修飾名を集める．`&mut Vec<String>`で再帰呼び出しに渡し，左右の表で同じ並びを使う．
- `catalog.table`の`SchemaError`は，`map_err`で`BindError::UndefinedTable`にした．メッセージは`SchemaError::UndefinedTable`と同じ`relation "T" does not exist`である．

`UPDATE`，`DELETE`，`INSERT`の名前解決も，`Scope::table(&schema.name, &schema.columns)`と`Scope::default()`を渡すように直した．

### 実行計画と`EXPLAIN`

```rust
#[test]
fn join_shows_its_kind_condition_and_both_children() {
    assert_eq!(
        explain(&plan_sql(
            "SELECT a.name FROM users a JOIN users b ON a.id = b.id CROSS JOIN users"
        )),
        vec![
            "Project [A.NAME]",
            "  NestedLoopJoin CROSS",
            "    NestedLoopJoin INNER (A.ID = B.ID)",
            "      SeqScan USERS A",
            "      SeqScan USERS B",
            "    SeqScan USERS",
        ]
    );
}
```

`plan_from`は，`BoundFrom`の表を`SeqScan`に，結合を`NestedLoopJoin`にする．`NestedLoopJoin`の列名は，左右の子の列名を並べたものである．

```rust
        BoundFrom::Join {
            left,
            right,
            kind,
            condition,
        } => {
            let left = plan_from(left);
            let right = plan_from(right);
            let mut columns = left.columns().to_vec();
            columns.extend_from_slice(right.columns());
            PlanNode::NestedLoopJoin {
                left: Box::new(left),
                right: Box::new(right),
                kind: *kind,
                condition: condition.clone(),
                columns,
            }
        }
```

`columns`は`&[String]`を返すので，結合の列名を2つの子からその場で作って返すことはできない．`NestedLoopJoin`は列名をフィールドに持つ．
`SeqScan`の列名は修飾名になったので，`EXPLAIN`の既存の期待値は`Project [USERS.NAME]`，`Filter (USERS.ID > 1)`に変わった．

### `NestedLoopJoin`

```rust
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
```

`CROSS JOIN`の項目から始めて，内部結合，左外部結合の順に状態を増やした．

```rust
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
```

- `current`は外側の今の行，`position`は内側の次に照らし合わせる位置，`matched`は今の外側の行に相手があったかである．条件を満たす組を返すと，この3つを覚えたまま戻る．
- 外側の今の行がなければ，外側の`next`で次の行を読む．`let ... else`の`else`は`continue`か`return`で必ず抜ける．
- 照らし合わせ終えた外側の行は，`take`で`current`から取り出す．`LEFT JOIN`で相手がなければ，その行に`NULL`を加えて返す．
- `CROSS JOIN`は条件を持たないので，`satisfies`は常に`true`を返す．
- 条件の評価のエラーの句の名前は，PostgreSQLと同じ`JOIN/ON`にした．

`join.rs`の残りの項目は，ここまでの実装で通る．

## 11-6 振り返り

1. 字句解析から演算子までの各段階に項目があるかを比べる．
2. 外側の行ごとに内側の演算子を作り直すと，内側が`Sort`なら外側の行の数だけ並べ替える．模範解答は内側を1度だけ読むので，並べ替えも1度で済む．その代わり，内側の行をすべて手元に置くので，内側が大きいとメモリーを使う．
3. `Expr::Column { table: Option<String>, name: String }`にまとめると，列の名前を扱う`match`の腕が1つで済む．一方，Iteration 7からの`Expr::Column("A".to_string())`と書いたテストや，`column_name`，`sort_source`の`match`をすべて書き換えることになる．別の列挙子にすれば，既存のコードとテストはそのまま動く．
4. `emp e LEFT JOIN dept d ON e.dept = d.code AND d.title = 'Operations'`は4行を返し，`Operations`以外の部署の社員は部署名が`NULL`になる．`... ON e.dept = d.code WHERE d.title = 'Operations'`は，`NULL`で埋めた行も`WHERE`で消えるので1行を返す．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 11-7 発展課題

解答例である．キーワード`RIGHT`と`JoinKind::Right`を加え，`join_kind`で`RIGHT [OUTER] JOIN`を読む．
`NestedLoopJoin`は，内側の行ごとに相手があったかを`right_matched`に覚える．外側の行を読み終えたら，相手のなかった内側の行に，外側の列の数だけ`NULL`を前に付けて返す．

```rust
    fn next_unmatched_right(&mut self) -> Option<Row> {
        if self.kind != JoinKind::Right {
            return None;
        }
        let right_rows = self.right_rows.as_ref()?;
        while self.unmatched_position < right_rows.len() {
            let index = self.unmatched_position;
            self.unmatched_position += 1;
            if !self.right_matched[index] {
                let mut row = vec![Value::Null; self.left_width];
                row.extend_from_slice(&right_rows[index]);
                return Some(row);
            }
        }
        None
    }
```

- `next`の中で外側の行がなくなったら，`Ok(None)`の代わりに`Ok(self.next_unmatched_right())`を返す．
- 条件を満たす組を返すときに，`self.right_matched[self.position - 1] = true`で内側の行に印を付ける．
- 最初の`next`で内側の行を読んだあと，`right_matched`を`vec![false; rows.len()]`で用意する．
- `new`に外側の列の数`left_width`を加え，`build`は`left.columns().len()`を渡す．`EXPLAIN`は`NestedLoopJoin RIGHT`と表示する．

```rust
#[test]
fn right_join_keeps_right_rows_without_a_match() {
    let mut db = database();
    db.execute("INSERT INTO dept VALUES ('hr', 'Human Resources')")
        .unwrap();
    let result = query(
        &mut db,
        "SELECT e.name, d.title FROM emp e RIGHT JOIN dept d ON e.dept = d.code",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![varchar("Sato"), varchar("Development")],
            vec![varchar("Suzuki"), varchar("Development")],
            vec![varchar("Tanaka"), varchar("Operations")],
            vec![Value::Null, varchar("Human Resources")],
        ]
    );
}
```
