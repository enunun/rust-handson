# Iteration 8：更新，削除，制約(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 8-1 準備

引き継いだ176個のテストがすべて通り，REPLで`UPDATE`が構文エラーになれば準備は終わりである．

```text
ERROR:  syntax error at or near "UPDATE"
```

## 8-2 文法と概念

課題の解答例である．

```rust
use std::collections::HashSet;

pub fn clamp_negatives(numbers: &mut [i32]) {
    for n in numbers.iter_mut() {
        if *n < 0 {
            *n = 0;
        }
    }
}

pub fn has_duplicate(words: &[&str]) -> bool {
    let mut seen = HashSet::new();
    words.iter().any(|word| !seen.insert(word))
}

pub fn gaps_from_min(numbers: &mut [i32]) {
    let min = *numbers.iter().min().unwrap();
    for n in numbers.iter_mut() {
        *n -= min;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let mut numbers = vec![3, -1, 0, -5];
        clamp_negatives(&mut numbers);
        assert_eq!(numbers, vec![3, 0, 0, 0]);

        let mut words = vec!["a", "", "b", ""];
        words.retain(|word| !word.is_empty());
        assert_eq!(words, vec!["a", "b"]);

        assert!(has_duplicate(&["a", "b", "a"]));
        assert!(!has_duplicate(&["a", "b"]));

        let mut numbers = vec![5, 2, 9];
        gaps_from_min(&mut numbers);
        assert_eq!(numbers, vec![3, 0, 7]);
    }
}
```

- 要素を書き換えるだけなら，引数は`&mut Vec<i32>`でなく`&mut [i32]`にする．`&mut Vec<i32>`と書くと，`cargo clippy`が`ptr_arg`でスライスにするよう求める．`&mut Vec<i32>`は`&mut [i32]`として渡せる．
- `has_duplicate`の`any`は，クロージャが真を返した時点で止まる．`insert`が`false`を返したら，その文字列は2度目である．

課題4で，最小の要素を`for`の中で求めると，次のエラーになる．

```text
error[E0502]: cannot borrow `*numbers` as immutable because it is also borrowed as mutable
 --> src/lib.rs:3:20
  |
2 |     for n in numbers.iter_mut() {
  |              ------------------
  |              |
  |              mutable borrow occurs here
  |              mutable borrow later used here
3 |         let min = *numbers.iter().min().unwrap();
  |                    ^^^^^^^ immutable borrow occurs here
```

`iter_mut`の借用はループの間ずっと続くので，その中で`numbers.iter()`を呼べない．最小の要素をループの前に求めれば(計算)，ループでは書き換えるだけになる(変更)．

## 8-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 構文解析，カタログ，行の変更と制約の検査(`exec::dml`)，表示，結合テストの順に並べた．
- 既存テストへの影響は，構文解析の期待値の`ColumnDef`と，カタログと名前解決のテストの補助関数の`Column`である．どちらも，新しいフィールドを加えるだけで，期待値の意味は変わらない．
- `exec::dml`の単体テストでは，`ID INTEGER PRIMARY KEY, NAME VARCHAR(10) NOT NULL, EMAIL VARCHAR(10) UNIQUE`にあたる`TableSchema`と3行の表を直接作った．`EMAIL`が`NULL`の行を2つ入れておくと，「`NULL`は重なっても違反にならない」ことを確かめられる．
- 「何も変えない」ことは，エラーのあとの表が補助関数`table()`の返す行と等しいことで確かめる．
- 評価のエラーが途中の行で起きる例として，`10 / (ID - 2) > 0`を使った．`ID`が1の行は評価でき，2の行で0による除算になる．
- 結合テストは，機能ごとに`dml`(`UPDATE`と`DELETE`)，`constraints`(制約)，`tables`(`DROP TABLE`)に分けた．

## 8-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | `exec::dml`と，`database`，`error`からの依存，`exec::dml`から`catalog`，`exec::eval`，`plan::binder`，`value`，`error`への依存を加えた | 行の変更と制約の検査を`database`から分けた |
| `code-types.md` | `ColumnConstraint`，`UniqueConstraint`，`ConstraintError`，`Update`，`Assignment`，`Delete`，`DropTable`を加え，`Statement`，`ColumnDef`，`Column`，`TableSchema`，`Catalog`，`StatementResult`，`SchemaError`，`SqlState`を更新した | 制約と新しい文を表す型ができた |
| `code-sequence.md` | `UPDATE`の図を加えた．`INSERT`と`DELETE`の説明を図の下に書いた | 新しい行の計算，制約の検査，表の置き換えの順序が，このIterationの中心である |

- `exec::dml`と`error`は互いに参照する．`exec::dml`の関数は，評価，型の変換，制約のどのエラーも`?`で`Error`にして返す．`error`は`ConstraintError`を`Error`に変換する．
- 構文木の`ColumnConstraint`は，書かれた制約をそのまま並べる．カタログでは，検査しやすい形で持つ．`NOT NULL`と`PRIMARY KEY`は`Column::nullable`で，`PRIMARY KEY`と`UNIQUE`は`UniqueConstraint`で表す．

## 8-5 テスト駆動の実装

### 制約と新しい文の構文

```rust
#[test]
fn column_constraints() {
    assert_eq!(
        statement("CREATE TABLE t (a INTEGER PRIMARY KEY, b INTEGER NOT NULL UNIQUE)"),
        Ok(Statement::CreateTable(CreateTable {
            name: "T".to_string(),
            columns: vec![
                ColumnDef {
                    name: "A".to_string(),
                    data_type: DataType::Integer,
                    constraints: vec![ColumnConstraint::PrimaryKey]
                },
                ColumnDef {
                    name: "B".to_string(),
                    data_type: DataType::Integer,
                    constraints: vec![ColumnConstraint::NotNull, ColumnConstraint::Unique]
                },
            ]
        }))
    );
}
```

キーワード`UPDATE`，`SET`，`DELETE`，`DROP`，`PRIMARY`，`KEY`，`UNIQUE`を加え，`ColumnDef`に`constraints`を加えた．
既存の`CREATE TABLE`の項目の期待値には，`constraints: vec![]`を加えた．

```rust
fn column_def(input: &mut Tokens<'_>) -> ModalResult<ColumnDef> {
    (identifier, data_type, repeat(0.., column_constraint))
        .map(to_column_def)
        .parse_next(input)
}

fn column_constraint(input: &mut Tokens<'_>) -> ModalResult<ColumnConstraint> {
    alt((
        (
            literal(keyword(Keyword::Not)),
            literal(keyword(Keyword::Null)),
        )
            .value(ColumnConstraint::NotNull),
        (
            literal(keyword(Keyword::Primary)),
            literal(keyword(Keyword::Key)),
        )
            .value(ColumnConstraint::PrimaryKey),
        literal(keyword(Keyword::Unique)).value(ColumnConstraint::Unique),
    ))
    .parse_next(input)
}
```

`.value`は値を複製するので，`ColumnConstraint`に`Clone`を導出した．

```rust
#[test]
fn update_with_assignments_and_where() {
    assert_eq!(
        statement("UPDATE t SET a = a + 1, b = 'x' WHERE a > 1"),
        Ok(Statement::Update(Update {
            table: "T".to_string(),
            assignments: vec![
                Assignment {
                    column: "A".to_string(),
                    value: arithmetic(ArithmeticOp::Add, column("A"), int(1))
                },
                Assignment {
                    column: "B".to_string(),
                    value: string("x")
                },
            ],
            filter: Some(comparison(ComparisonOp::Gt, column("A"), int(1)))
        }))
    );
}
```

```rust
fn update(input: &mut Tokens<'_>) -> ModalResult<Update> {
    preceded(
        literal(keyword(Keyword::Update)),
        cut_err((
            identifier,
            preceded(
                literal(keyword(Keyword::Set)),
                separated(1.., assignment, literal(Token::Comma)),
            ),
            opt(preceded(literal(keyword(Keyword::Where)), expr)),
        )),
    )
    .map(|(table, assignments, filter)| Update {
        table,
        assignments,
        filter,
    })
    .parse_next(input)
}

fn assignment(input: &mut Tokens<'_>) -> ModalResult<Assignment> {
    (identifier, preceded(literal(Token::Eq), expr))
        .map(|(column, value)| Assignment { column, value })
        .parse_next(input)
}
```

`cut_err`があるので，`UPDATE t SET WHERE a > 1`は`WHERE`の位置(14文字目)の構文エラーになる．
`DELETE FROM`と`DROP TABLE`も同じ形で書き，`statement`の`alt`に加えた．

```rust
fn delete(input: &mut Tokens<'_>) -> ModalResult<Delete> {
    preceded(
        (
            literal(keyword(Keyword::Delete)),
            literal(keyword(Keyword::From)),
        ),
        cut_err((
            identifier,
            opt(preceded(literal(keyword(Keyword::Where)), expr)),
        )),
    )
    .map(|(table, filter)| Delete { table, filter })
    .parse_next(input)
}
```

新しい`Statement`の列挙子を加えると，`Database::execute`の`match`がすべての列挙子を扱っていないというエラーになる．
`Database`を実装するまでは，`todo!()`を返す腕を置いて先へ進める．

### カタログ：制約の情報と表の削除

```rust
#[test]
fn dropped_table_can_no_longer_be_found() {
    let mut catalog = Catalog::default();
    catalog.create_table(users()).unwrap();
    assert_eq!(catalog.drop_table("USERS"), Ok(()));
    assert_eq!(
        catalog.table("USERS"),
        Err(SchemaError::UndefinedTable {
            table: "USERS".to_string()
        })
    );
}
```

`Column`に`nullable`を，`TableSchema`に`unique_constraints`を加え，テストの補助関数を直した．

```rust
pub fn drop_table(&mut self, name: &str) -> Result<(), SchemaError> {
    match self.tables.remove(name) {
        Some(_) => Ok(()),
        None => Err(SchemaError::UndefinedTableToDrop {
            table: name.to_string(),
        }),
    }
}
```

`HashMap::remove`は，消した値を`Option`で返す．
PostgreSQLは，`DROP TABLE`の対象がないときだけ`relation`でなく`table`という語を使う．メッセージを分けるため，`UndefinedTable`とは別の列挙子にした．

### 制約の検査

```rust
#[test]
fn duplicate_value_violates_the_unique_constraint() {
    let mut rows = table();
    rows.push(row(9, "dave", Some("a@x")));
    assert_eq!(
        check_constraints(&users(), &rows),
        Err(ConstraintError::Unique {
            constraint: "USERS_EMAIL_KEY".to_string()
        })
    );
}
```

`src/exec/dml.rs`を作り，`ConstraintError`と`check_constraints`を書いた．
`NOT NULL`の項目は`any`で，一意性の項目は`HashSet`で通した．

```rust
fn check_constraints(schema: &TableSchema, rows: &[Row]) -> Result<(), ConstraintError> {
    for (index, column) in schema.columns.iter().enumerate() {
        if !column.nullable && rows.iter().any(|row| row[index].is_null()) {
            return Err(ConstraintError::NotNull {
                table: schema.name.clone(),
                column: column.name.clone(),
            });
        }
    }
    for constraint in &schema.unique_constraints {
        let mut seen = HashSet::new();
        for row in rows {
            let value = &row[constraint.column];
            if !value.is_null() && !seen.insert(value) {
                return Err(ConstraintError::Unique {
                    constraint: constraint.name.clone(),
                });
            }
        }
    }
    Ok(())
}
```

`seen`は`HashSet<&Value>`になる．`Value`に`Eq`と`Hash`を導出した．
`NULL`は`insert`する前に除くので，`NULL`がいくつあっても違反にならない．

### `insert`

```rust
#[test]
fn insert_violating_a_constraint_adds_no_rows() {
    let mut rows = table();
    let new_rows = vec![row(4, "dave", None), row(1, "eve", None)];
    let error = insert(&users(), &mut rows, new_rows).unwrap_err();
    assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
    assert_eq!(rows, table());
}
```

1つ前の項目(行を加えて数を返す)は，`rows.extend(new_rows)`で通した．
制約の項目では，表の行の複製に新しい行を加えて検査し，違反がなければ置き換える形にした．

```rust
pub fn insert(schema: &TableSchema, rows: &mut Vec<Row>, new_rows: Vec<Row>) -> Result<usize, Error> {
    let count = new_rows.len();
    let mut after = rows.clone();
    after.extend(new_rows);
    check_constraints(schema, &after)?;
    *rows = after;
    Ok(count)
}
```

`*rows = after`は，参照の先の`Vec`をまるごと置き換える．置き換えるので，引数は`&mut [Row]`でなく`&mut Vec<Row>`である．
`error`に`SqlState::NotNullViolation`，`UniqueViolation`と`From<ConstraintError>`を加えた．これで，`?`が`ConstraintError`を`Error`に変換する．

### `update`

```rust
#[test]
fn update_evaluates_assignments_on_the_row_before_the_update() {
    let mut rows = vec![row(1, "a", Some("b"))];
    let assignments = vec![(1, column(2)), (2, column(1))];
    update(&users(), &mut rows, &assignments, &None).unwrap();
    assert_eq!(rows, vec![row(1, "b", Some("a"))]);
}
```

最初の項目(条件を満たす行に代入する)は，`iter_mut`で行を直接書き換えて通した．
制約に違反したら1行も変えない項目のために，書き換えのたびに制約を検査しようとすると，次のエラーになる．

```rust
    let mut count = 0;
    for row in rows.iter_mut() {
        if let Some(new_row) = updated_row(schema, row, assignments, filter)? {
            *row = new_row;
            count += 1;
            check_constraints(schema, rows)?;
        }
    }
```

```text
error[E0502]: cannot borrow `*rows` as immutable because it is also borrowed as mutable
  --> src/exec/dml.rs:45:39
   |
41 |     for row in rows.iter_mut() {
   |                ---------------
   |                |
   |                mutable borrow occurs here
   |                mutable borrow later used here
...
45 |             check_constraints(schema, rows)?;
   |                                       ^^^^ immutable borrow occurs here
```

仮にこのコードが通っても，検査で違反を見つけた時点で，前の行はすでに書き換わっている．また，文の途中の一時的な重なりも違反にしてしまう．
そこで，計算と変更を分けた．

```rust
pub fn update(
    schema: &TableSchema,
    rows: &mut Vec<Row>,
    assignments: &[(usize, BoundExpr)],
    filter: &Option<BoundExpr>,
) -> Result<usize, Error> {
    let changes = rows
        .iter()
        .map(|row| updated_row(schema, row, assignments, filter))
        .collect::<Result<Vec<Option<Row>>, Error>>()?;
    let count = changes.iter().filter(|change| change.is_some()).count();
    let mut after = rows.clone();
    for (row, change) in after.iter_mut().zip(changes) {
        if let Some(new_row) = change {
            *row = new_row;
        }
    }
    check_constraints(schema, &after)?;
    *rows = after;
    Ok(count)
}

fn updated_row(
    schema: &TableSchema,
    row: &[Value],
    assignments: &[(usize, BoundExpr)],
    filter: &Option<BoundExpr>,
) -> Result<Option<Row>, Error> {
    if !matches_filter(filter, row)? {
        return Ok(None);
    }
    let mut new_row = row.to_vec();
    for (index, expr) in assignments {
        new_row[*index] = schema.columns[*index].assign(eval(expr, row)?)?;
    }
    Ok(Some(new_row))
}
```

- 計算の段階では，`rows`を`&`で読むだけである．評価のエラーはここで返るので，表は変わらない．
- `updated_row`は，代入の式を元の行`row`で評価し，結果を`new_row`に書き込む．`new_row`で評価すると，`SET a = b, b = a`の2つ目の代入が書き換えたあとの`a`を読んでしまう．
- 変更の段階では，複製`after`を`iter_mut`で書き換える．制約はすべての行を書き換えたあとに検査するので，`ID = ID + 1`は成功する．

`matches_filter`は，`database`から`exec::eval`に移した(Refactor)．戻り値は`Result<bool, EvalError>`にした．

```rust
pub fn matches_filter(filter: &Option<BoundExpr>, row: &[Value]) -> Result<bool, EvalError> {
    match filter {
        Some(filter) => eval_condition(filter, row, "WHERE"),
        None => Ok(true),
    }
}
```

### `delete`

```rust
#[test]
fn error_in_the_condition_leaves_the_table_unchanged() {
    let mut rows = table();
    let assignments = vec![(1, constant(Value::Varchar("zed".to_string())))];
    let filter = divides_by_zero_at_id_2();
    let error = update(&users(), &mut rows, &assignments, &filter).unwrap_err();
    assert_eq!(error.sqlstate(), SqlState::DivisionByZero);
    let error = delete(&mut rows, &filter).unwrap_err();
    assert_eq!(error.sqlstate(), SqlState::DivisionByZero);
    assert_eq!(rows, table());
}
```

`retain`のクロージャは`bool`を返すので，条件の評価のエラーを返せない．先にすべての行の条件を評価し，エラーがなければ`retain`で消す．

```rust
pub fn delete(rows: &mut Vec<Row>, filter: &Option<BoundExpr>) -> Result<usize, Error> {
    let targets = rows
        .iter()
        .map(|row| matches_filter(filter, row))
        .collect::<Result<Vec<bool>, _>>()?;
    let before = rows.len();
    let mut targets = targets.into_iter();
    rows.retain(|_| !targets.next().expect("one flag for each row"));
    Ok(before - rows.len())
}
```

`retain`は先頭の行から順にクロージャを呼ぶので，`targets`の真偽値と行が1つずつ対応する．

### `Database`とコマンドタグ

```rust
#[test]
fn update_changes_matching_rows_and_counts_them() {
    let mut db = database();
    assert_eq!(
        db.execute("UPDATE users SET age = age + 1, name = 'bobby' WHERE id = 2 OR id = 3"),
        Ok(StatementResult::Update { count: 2 })
    );
    // 表の行を SELECT で確かめる
}
```

`Database`は，構文木の名前を解決してから`exec::dml`を呼ぶ．

```rust
fn update(&mut self, update: &Update) -> Result<StatementResult, Error> {
    let schema = self.catalog.table(&update.table)?;
    let assignments = bind_assignments(schema, &update.assignments)?;
    let filter = bind_filter(&update.filter, &schema.columns)?;
    let rows = self
        .rows
        .get_mut(&update.table)
        .expect("every table in the catalog has its rows");
    let count = dml::update(schema, rows, &assignments, &filter)?;
    Ok(StatementResult::Update { count })
}
```

`schema`は`self.catalog`を，`rows`は`self.rows`を借用する．別のフィールドなので，同時に借用できる．
3つの文で同じ`get_mut`を書くのを避けて`&mut self`のメソッド`table_rows`にまとめると，次のエラーになる．

```text
error[E0502]: cannot borrow `*self` as mutable because it is also borrowed as immutable
   --> src/database.rs:107:20
    |
104 |         let schema = self.catalog.table(&update.table)?;
    |                      ------------ immutable borrow occurs here
...
107 |         let rows = self.table_rows(&update.table);
    |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ mutable borrow occurs here
108 |         let count = dml::update(schema, rows, &assignments, &filter)?;
    |                                 ------ immutable borrow later used here
```

メソッドは`self`全体を`&mut`で借用するので，`self.catalog`の借用と衝突する．フィールドを直接書く形のままにした．

代入の名前解決では，列の番号を引き，同じ列への2度目の代入をエラーにする．

```rust
fn bind_assignments(
    schema: &TableSchema,
    assignments: &[Assignment],
) -> Result<Vec<(usize, BoundExpr)>, Error> {
    let mut bound = Vec::new();
    for assignment in assignments {
        let index = schema.column_index(&assignment.column)?;
        if bound.iter().any(|(column, _)| *column == index) {
            return Err(SchemaError::DuplicateAssignment {
                column: assignment.column.clone(),
            }
            .into());
        }
        bound.push((index, bind(&assignment.value, &schema.columns)?));
    }
    Ok(bound)
}
```

ない列への代入は，`column_index`が`UndefinedColumn`を返すので，`column "EMAIL" of relation "USERS" does not exist`になる．
`WHERE`の名前解決は`SELECT`と同じなので，`bind_filter`に分けて3つの文で使う．

`format`には，コマンドタグ`UPDATE n`，`DELETE n`，`DROP TABLE`を加えた．
`dml.rs`と`tables.rs`の残りの項目は，ここまでの実装で通る．

### `CREATE TABLE`の制約

```rust
#[test]
fn duplicate_primary_key_is_rejected_and_no_row_is_inserted() {
    let mut db = database();
    let err = db
        .execute("INSERT INTO users VALUES (3, 'carol', NULL), (1, 'dave', NULL)")
        .unwrap_err();
    assert_eq!(err.sqlstate().code(), "23505");
    assert_eq!(
        err.to_string(),
        "duplicate key value violates unique constraint \"USERS_PKEY\""
    );
    assert_eq!(
        ids(&mut db),
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
}
```

列の定義の制約から，`Column::nullable`と`UniqueConstraint`を作る．

```rust
fn table_schema(create: CreateTable) -> Result<TableSchema, SchemaError> {
    let mut columns = Vec::new();
    let mut unique_constraints = Vec::new();
    let mut has_primary_key = false;
    for (index, column) in create.columns.into_iter().enumerate() {
        let primary_key = column.constraints.contains(&ColumnConstraint::PrimaryKey);
        if primary_key {
            if has_primary_key {
                return Err(SchemaError::MultiplePrimaryKeys { table: create.name });
            }
            has_primary_key = true;
            unique_constraints.push(UniqueConstraint {
                name: format!("{}_PKEY", create.name),
                column: index,
            });
        } else if column.constraints.contains(&ColumnConstraint::Unique) {
            unique_constraints.push(UniqueConstraint {
                name: format!("{}_{}_KEY", create.name, column.name),
                column: index,
            });
        }
        let not_null = column.constraints.contains(&ColumnConstraint::NotNull);
        columns.push(Column {
            name: column.name,
            data_type: column.data_type,
            nullable: !primary_key && !not_null,
        });
    }
    Ok(TableSchema {
        name: create.name,
        columns,
        unique_constraints,
    })
}
```

- `create.columns.into_iter()`は`create`の`columns`だけをムーブする．`create.name`はそのまま使える．
- 同じ列に`PRIMARY KEY`と`UNIQUE`を書いても，`else if`なので主キーの制約だけを作る．どちらも同じことを検査するからである．

`SqlState::InvalidTableDefinition`(`42P16`)を加えた．`constraints.rs`の残りの項目は，ここまでの実装で通る．

## 8-6 振り返り

1. 構文解析，カタログ，制約の検査，`insert`，`update`，`delete`のそれぞれに，成功する項目と「何も変えない」項目があるかを比べる．
2. 新しい行だけを検査するなら，`INSERT`では「新しい行どうし」と「新しい行と既存の行」を比べる．`UPDATE`では，書き換えた行が書き換えていない行や，ほかの書き換えた行と重ならないかを比べる．書き換える前の値と比べてはいけない(`ID = ID + 1`が失敗する)．比べる組み合わせが増える代わりに，表全体の複製は要らなくなる．Iteration 17では，インデックスを引いて重なりを調べる．
3. `Column`に`constraints: Vec<ColumnConstraint>`を持たせると，検査のコードは「`NotNull`か`PrimaryKey`を含むか」を毎回調べることになり，`PRIMARY KEY`が`NOT NULL`を含むという規則が検査のコードに散らばる．模範解答では，`CREATE TABLE`を処理する`table_schema`でこの規則を1度だけ適用し，カタログは検査に使う形(`nullable`，`UniqueConstraint`)で持つ．構文木は書かれたとおりに，カタログは意味のとおりに表している．
4. `exec::dml`の単体テストが失敗したら，原因は`update`と制約の検査の中にある．結合テストだけが失敗したら，構文解析，名前解決，`table_schema`など，`exec::dml`に渡すまでの段階を疑う．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 8-7 発展課題

解答例である．表の要素を「列の定義か表制約」の直和型として読み，`CREATE TABLE`の構文木に分けて入れる．

```rust
/// 表制約．`PRIMARY KEY (列)`か`UNIQUE (列)`である．
#[derive(Debug, PartialEq)]
pub struct TableConstraint {
    pub kind: ColumnConstraint,
    pub column: String,
}
```

```rust
/// 表の要素．列の定義か表制約である．
enum TableElement {
    Column(ColumnDef),
    Constraint(TableConstraint),
}

fn table_element(input: &mut Tokens<'_>) -> ModalResult<TableElement> {
    alt((
        table_constraint.map(TableElement::Constraint),
        column_def.map(TableElement::Column),
    ))
    .parse_next(input)
}

fn table_constraint(input: &mut Tokens<'_>) -> ModalResult<TableConstraint> {
    (
        alt((
            (
                literal(keyword(Keyword::Primary)),
                literal(keyword(Keyword::Key)),
            )
                .value(ColumnConstraint::PrimaryKey),
            literal(keyword(Keyword::Unique)).value(ColumnConstraint::Unique),
        )),
        cut_err(delimited(
            literal(Token::LParen),
            identifier,
            literal(Token::RParen),
        )),
    )
        .map(|(kind, column)| TableConstraint { kind, column })
        .parse_next(input)
}

fn to_create_table((name, elements): (String, Vec<TableElement>)) -> CreateTable {
    let mut columns = Vec::new();
    let mut constraints = Vec::new();
    for element in elements {
        match element {
            TableElement::Column(column) => columns.push(column),
            TableElement::Constraint(constraint) => constraints.push(constraint),
        }
    }
    CreateTable {
        name,
        columns,
        constraints,
    }
}
```

`create_table`の`separated`で`column_def`の代わりに`table_element`を読む．
`table_schema`の最初で，表制約を該当する列の`constraints`に加える．あとの処理は列制約と同じになる．

```rust
fn table_schema(mut create: CreateTable) -> Result<TableSchema, SchemaError> {
    for constraint in create.constraints {
        let column = create
            .columns
            .iter_mut()
            .find(|column| column.name == constraint.column)
            .ok_or_else(|| SchemaError::UndefinedKeyColumn {
                column: constraint.column.clone(),
            })?;
        column.constraints.push(constraint.kind);
    }
    // ここから後は列制約の処理と同じ
```

```rust
#[test]
fn table_constraints_name_one_column() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER, b INTEGER, PRIMARY KEY (a), UNIQUE (b))")
        .unwrap();
    db.execute("INSERT INTO t VALUES (1, 1)").unwrap();
    let err = db.execute("INSERT INTO t VALUES (NULL, 2)").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::NotNullViolation);
    let err = db.execute("INSERT INTO t VALUES (2, 1)").unwrap_err();
    assert_eq!(
        err.to_string(),
        "duplicate key value violates unique constraint \"T_B_KEY\""
    );
    let err = db
        .execute("CREATE TABLE u (a INTEGER, UNIQUE (c))")
        .unwrap_err();
    assert_eq!(err.sqlstate().code(), "42703");
    assert_eq!(err.to_string(), "column \"C\" named in key does not exist");
}
```

- `find`は，クロージャが真を返す最初の要素を`Option`で返す．`iter_mut`から呼ぶと，要素への`&mut`が返るので，そのまま`push`できる．
- 引数を`mut create`にすると，受け取った値を関数の中で書き換えられる．
- `SchemaError::UndefinedKeyColumn`と，`42703`への変換を加える．
- 既存の`CREATE TABLE`の構文解析の期待値に`constraints: vec![]`を加える．
