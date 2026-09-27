# Iteration 17：インデックスの利用(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 17-1 準備

引き継いだ381個のテストがすべて通れば準備は終わりである．
Iteration 16までの`EXPLAIN`は，`WHERE`の条件を`SeqScan`の上の`Filter`で調べる．

## 17-2 文法と概念

課題の解答例である．`tests/`に置いた結合テストで確かめた．

```rust
use std::ops::Bound;

#[derive(Debug)]
pub enum Key {
    Int(i64),
    Text(String),
}

pub enum Keys {
    Ints(Vec<i64>),
    Texts(Vec<String>),
}

impl Keys {
    pub fn push(&mut self, key: Key) -> Result<(), String> {
        match (self, key) {
            (Keys::Ints(keys), Key::Int(n)) => keys.push(n),
            (Keys::Texts(keys), Key::Text(s)) => keys.push(s),
            (_, key) => return Err(format!("{key:?} does not match")),
        }
        Ok(())
    }
}

pub fn tighter_upper(a: Bound<i32>, b: Bound<i32>) -> Bound<i32> {
    match (&a, &b) {
        (Bound::Unbounded, _) => b,
        (_, Bound::Unbounded) => a,
        (Bound::Included(x) | Bound::Excluded(x), Bound::Included(y) | Bound::Excluded(y)) => {
            if y < x || (y == x && matches!(b, Bound::Excluded(_))) {
                b
            } else {
                a
            }
        }
    }
}

pub struct Words {
    names: Vec<String>,
    lengths: Vec<usize>,
}

impl Words {
    pub fn measure(&mut self) {
        let lengths = &mut self.lengths;
        let names = &self.names;
        lengths.clear();
        for name in names {
            lengths.push(name.chars().count());
        }
    }
}
```

- `Keys::push`の`key`は値で受け取るので，`Key::Text(s)`の`s`は`String`としてそのまま`Vec`に移せる．
- `tighter_upper`は，`a`と`b`を借りて`match`し，腕の中で`a`か`b`をそのまま返す．`match (&a, &b)`の借用は，比べ終えた時点で終わる．
- `measure`の`for name in names`の`name`は`&String`である．

## 17-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 構文，カタログ，`AnyIndex`，プランナー，演算子，行の変更，結合テストの順に並べた．
- プランナーのテストのカタログに，`ID`と`NAME`のインデックスを加えた．引き継いだ`operators_are_stacked_in_the_order_of_processing`の`WHERE id > 1`は`IndexScan`になるので，期待値を変えた．
- インデックスを使えない条件は，`OR`，`<>`，`INTEGER`の範囲を超える定数，`NULL`，結合の`WHERE`を1つのテストに並べた．
- `exec::dml`の`check_constraints`は表のすべての行を受け取っていた．一意性の検査をインデックスに移したので，`NOT NULL`の検査は`check_not_null`に分け，一意性はインデックスを作った表で`insert`と`update`を呼んで確かめる．
- 行を変えたあとのインデックスは，インデックスで引いた位置の行を読み，期待した行と比べて確かめた．
- 既存の結合テストは1つも変えていない．`PRIMARY KEY`の列を`WHERE`で比べる`EXPLAIN`は，引き継いだテストにはない．

## 17-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-container.md` | データディレクトリにインデックスのファイル`*.index`を加えた | インデックスをファイルに置く |
| `c4-component.md` | `index`と`exec::index_scan`を加え，`database`，`plan::planner`，`exec::build`，`exec::dml`，`error`からの依存を加えた | インデックスを作り，使い，更新するモジュールが増えた |
| `code-types.md` | `AnyIndex`，`ColumnIndex`，`IndexDef`，`IndexScan`，`CreateIndex`，`DropIndex`を加え，`Database`，`Catalog`，`PlanNode`，`StatementResult`，`SchemaError`を更新した | インデックスの型と文ができた |
| `code-sequence.md` | プランナーがインデックスを選び，`IndexScan`で行を読む流れを加えた．`UPDATE`の図の制約の検査を，インデックスで引く形にした | スキャン方法を選ぶようになり，一意性の検査が変わった |
| `layout.md` | カタログのファイルのインデックスの定義と，インデックスのファイルの名前を加えた | カタログの形式が変わった |

- `index`は子のモジュールを宣言するだけでなく`AnyIndex`を定義するので，Component図で`index`の境界の中にComponentとして描いた．
- `plan::planner`は`index`に依存しない．インデックスの定義(`IndexDef`)は`catalog`にあり，プランナーはどのインデックスを使うかを名前で決める．実際にB+木を開くのは`exec::build`である．

## 17-5 テスト駆動の実装

### 構文

`statement`の`alt`に`create_index`と`drop_index`を加えると，選択肢が10個になり，次のエラーになった．

```text
error[E0277]: the trait bound `(..., ..., ..., ..., ..., ..., ..., ..., ..., ...): Alt<_, _, _>` is not satisfied
  --> iterations/iteration-17/solution/src/sql/parser.rs:78:9
   |
78 |       alt((
   |  _____---_^
   | |     |
   | |     required by a bound introduced by this call
79 | |         values.map(Statement::Values),
80 | |         create_table.map(Statement::CreateTable),
81 | |         create_index.map(Statement::CreateIndex),
...  |
88 | |         preceded(literal(keyword(Keyword::Explain)), cut_err(select)).map(Statement::Explain),
89 | |     ))
   | |_____^ the trait `winnow::combinator::Alt<_, _, _>` is not implemented for `(..., ..., ..., ..., ..., ..., ..., ..., ..., ...)`
   |
   = help: the following other types implement trait `winnow::combinator::Alt<I, O, E>`:
             `(A,)` implements `winnow::combinator::Alt<I, O, E>`
             `(Alt2, Alt3)` implements `winnow::combinator::Alt<I, Output, Error>`
```

winnow 1.0の`alt`は，9個までの選択肢の組に実装されている．`CREATE`で始まる文と`DROP`で始まる文をそれぞれ`alt`にまとめ，選択肢を8個にした．

```rust
fn create(input: &mut Tokens<'_>) -> ModalResult<Statement> {
    alt((
        create_table.map(Statement::CreateTable),
        create_index.map(Statement::CreateIndex),
    ))
    .parse_next(input)
}
```

`create_table`は`CREATE TABLE`の2つのキーワードを読んでから`cut_err`するので，`CREATE INDEX`なら`TABLE`で失敗して`create_index`に進む．

### カタログ

`Catalog`は`indexes: HashMap<String, IndexDef>`を持つ．表とインデックスの名前は，`check_new_name`で両方の表を調べて重ならないようにした．

```rust
    pub fn create_table(&mut self, schema: TableSchema) -> Result<(), SchemaError> {
        let mut names = vec![&schema.name];
        names.extend(schema.unique_constraints.iter().map(|c| &c.name));
        for name in names {
            self.check_new_name(name)?;
        }
        // 列の名前の重なりを調べる
        for constraint in &schema.unique_constraints {
            self.indexes.insert(
                constraint.name.clone(),
                IndexDef {
                    name: constraint.name.clone(),
                    table: schema.name.clone(),
                    column: constraint.column,
                },
            );
        }
        self.tables.insert(schema.name.clone(), schema);
        Ok(())
    }
```

`drop_index`は，名前が表の一意性制約の名前と同じなら`IndexRequiredByConstraint`を返す．`drop_table`は，`retain`で表のインデックスの定義を消す．
カタログのファイルでは，表の定義のあとにインデックスの定義を名前の順に並べる．

### `AnyIndex`

列の型ごとの`BTree`を`enum`でまとめ，SQLの`Value`で使えるようにした．

```rust
pub enum AnyIndex<'a> {
    Integer(BTree<'a, i32>),
    BigInt(BTree<'a, i64>),
    Boolean(BTree<'a, bool>),
    Varchar(BTree<'a, String>),
}

impl<'a> AnyIndex<'a> {
    pub fn insert(&mut self, value: &Value, id: RowId) -> Result<(), BTreeError> {
        match (self, value) {
            (_, Value::Null) => Ok(()),
            (AnyIndex::Integer(tree), Value::Integer(n)) => tree.insert(*n, id),
            (AnyIndex::BigInt(tree), Value::BigInt(n)) => tree.insert(*n, id),
            (AnyIndex::Boolean(tree), Value::Boolean(b)) => tree.insert(*b, id),
            (AnyIndex::Varchar(tree), Value::Varchar(s)) => tree.insert(s.clone(), id),
            (_, value) => unreachable!("{value:?} does not match the type of the index"),
        }
    }

    pub fn range(
        &self,
        lower: Bound<&Value>,
        upper: Bound<&Value>,
    ) -> Result<Vec<RowId>, BTreeError> {
        match self {
            AnyIndex::Integer(tree) => row_ids(tree, lower.map(as_integer), upper.map(as_integer)),
            // BIGINT，BOOLEAN，VARCHARも同じ形
        }
    }
}
```

- `range`の結果は，`RangeIter`のキーを捨てた`RowId`の`Vec`である．`row_ids`は`K: IndexKey`の型引数で，4つの型の木に同じ処理を使う．
- `check_key`は，`VARCHAR`の値が`MAX_KEY_SIZE`を超えないかを調べる．行を変える前にすべての値を調べるために，`insert`とは別の関数にした．

### 一意性の検査とインデックスの更新

`exec::dml`の関数は`&mut [ColumnIndex<'_>]`を受け取る．`update`は，変える行の新しい値だけを検査する．

```rust
fn check_unique(
    schema: &TableSchema,
    indexes: &[ColumnIndex<'_>],
    rows: &[Row],
    replaced: &HashSet<RowId>,
) -> Result<(), Error> {
    for constraint in &schema.unique_constraints {
        let index = indexes
            .iter()
            .find(|index| index.column == constraint.column)
            .expect("every unique constraint has an index");
        let violation = ConstraintError::Unique {
            constraint: constraint.name.clone(),
        };
        let mut seen = HashSet::new();
        for row in rows {
            let value = &row[constraint.column];
            if value.is_null() {
                continue;
            }
            if !seen.insert(value) {
                return Err(violation.into());
            }
            let others = index.index.lookup(value)?;
            if others.iter().any(|id| !replaced.contains(id)) {
                return Err(violation.into());
            }
        }
    }
    Ok(())
}
```

- `seen`は，変える行どうしの重なりを見つける．
- インデックスで見つかった位置が`replaced`(書き換える前の行)のどれかなら，その行の値は変わるので違反としない．`UPDATE t SET id = id + 1`で，`2`の行を`3`にするとき，今の`3`の行も書き換える行なので違反にならない．
- `INSERT`は`replaced`に空の集合を渡す．

検査を終えてから，表とインデックスを変える．

```rust
    for ((id, old_row, new_row), tuple) in targets.iter().zip(&tuples) {
        let new_id = heap.update(*id, tuple)?;
        for index in indexes.iter_mut() {
            index.index.delete(&old_row[index.column], *id)?;
            index.index.insert(&new_row[index.column], new_id)?;
        }
    }
```

`HeapFile::update`は行を別のページに移すことがあるので，列の値が変わらなくても，すべてのインデックスの項目を新しい位置に置き直す．

### プランナーと`IndexScan`

`plan`はカタログを受け取り，`plan_scan`で`FROM`を読む演算子を決める．`choose_index`は，表のインデックスごとに，`AND`で分けた条件から範囲を決められる条件を集める．

```rust
            let Some((target, op, value)) = column_comparison(conjunct) else {
                continue;
            };
            if target != index.column || value.is_null() {
                continue;
            }
            let Ok(value) = column.assign(value.clone()) else {
                continue;
            };
            match op {
                ComparisonOp::Eq => {
                    choice.lower = tighter_lower(choice.lower, Bound::Included(value.clone()));
                    choice.upper = tighter_upper(choice.upper, Bound::Included(value));
                    choice.equality = true;
                }
                // <，<=，>，>=は，下限か上限の片方を狭める
            }
            choice.used.push(position);
```

- `column_comparison`は，`列 比較 定数`と`定数 比較 列`を，列から見た比較演算子に揃える．`3 > id`は`id < 3`になる．
- 定数は`Column::assign`で列の型にする．`INTEGER`の列に`3000000000`を比べる条件は，変換できないのでインデックスを使わない．`Filter`で比べれば，これまでどおり偽になる．
- 範囲を決めた条件は`IndexScan`の`conditions`に，残りは`and_all`でつないで`Filter`に置く．

`exec::build`は，`AnyIndex::open`でインデックスを開き，キーの範囲の`RowId`ごとに`HeapFile::get`でタプルを読む．`IndexScan`の演算子は，その行を順に返す．

### `Database`

`Database`は，インデックスの名前ごとの`BufferPool<Box<dyn DiskManager>>`を持つ．`AnyIndex`はプールを借りるので，`Database`には持たせず，文を実行するたびに`open_indexes`で開く．

```rust
        let heap = self
            .tables
            .get_mut(&insert.table)
            .expect("every table in the catalog has its heap file");
        let mut indexes = open_indexes(&self.catalog, &self.indexes, schema)?;
        let count = dml::insert(schema, heap, &mut indexes, new_rows)?;
```

`open_indexes`を`&self`のメソッドにすると，`self.tables`を`&mut`で借りている間に`self`全体を借りることになり，借用のエラーになる．フィールドを受け取る関数にした．

`CREATE INDEX`は，カタログに定義を加えてから，プールに木を作って表の行を入れる．入れられない行があれば，カタログの定義とファイルを消してエラーを返す．
`BufferPool`には`Debug`がなかったので，枠の数とページの数を書く`Debug`を実装した．`Database`が`#[derive(Debug)]`を持つからである．

結合テストの`tests/index.rs`は，ここまでの実装で通る．

## 17-6 振り返り

1. 範囲を決める条件と残りの条件の組み合わせ，インデックスを使えない条件，行を変えたあとのインデックスの項目を確かめる項目があるかを比べる．
2. `enum`では，キーの型を加えると，`AnyIndex`の列挙子と，`create`，`open`，`insert`，`delete`，`range`の`match`をすべて直す．操作を加えるときは，メソッドを1つ加えるだけでよい．`Box<dyn Index>`なら，キーの型を加えるときは新しい型に`Index`を実装するだけで，操作を加えるときはトレイトとすべての実装を直す．SQLの型は4つに決まっていて，操作の方が増えやすいので，`enum`にした．
3. Iteration 8の方法は，表のすべてのページを読む．100万行なら数万ページである．インデックスで引くと，読むのは木の高さの数(3か4)のページだけで，表の大きさにほとんどよらない．
4. `IndexScan`は，`RowId`ごとにその行のページを取得する．同じページの行でも，行の数だけ取得し直し，ページの順に読むとも限らない．ほとんどの行が合うなら，ページを順に1回ずつ読む全件走査の方が少ない手間で済む．
5. `NULL`をキーに符号化する必要がある．たとえば，キーの前に「`NULL`か」を表す1バイトを置き，`NULL`でない値を`00`，`NULL`を`01`で始めれば，`NULL`はすべての値の後ろに並ぶ．一意性の検査では，`NULL`のキーの項目を重なりとみなさないようにする．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 17-7 発展課題

解答例である．プランナーで，`Sort`のキーをインデックスが満たすかを調べる関数を作る．

```rust
fn ordered_by_index(
    input: &PlanNode,
    exprs: &[BoundExpr],
    keys: &[SortKey],
    catalog: &Catalog,
) -> bool {
    let [key] = keys else {
        return false;
    };
    if key.order.descending {
        return false;
    }
    let scan = match input {
        PlanNode::Filter { input, .. } => input,
        other => other,
    };
    let PlanNode::IndexScan { index, .. } = scan else {
        return false;
    };
    let Ok(index) = catalog.index(index) else {
        return false;
    };
    let BoundExpr::Column(position) = key.expr else {
        return false;
    };
    exprs[position] == BoundExpr::Column(index.column)
}
```

`plan`で`Project`を作る前に呼び，満たすなら`keys`を空にする．

```rust
    if ordered_by_index(&node, &exprs, &keys, catalog) {
        keys.clear();
    }
```

- 並べ替えのキーは`Project`の結果の列を指すので，`exprs[position]`がインデックスの列そのもの(`BoundExpr::Column`)かを比べる．選択項目にない列で並べ替えるときも，隠れた列として`exprs`にある．
- `IndexScan`の行は`NULL`を含まないので，`NULLS FIRST`か`NULLS LAST`かによらない．
- 集約があれば，`Project`の下は`HashAggregate`か`HAVING`の`Filter`になるので，`Sort`を残す．

```rust
#[test]
fn sort_is_skipped_when_the_index_gives_the_order() {
    // EMP を作り，EMP_SALARY を作ってから
    assert_eq!(
        lines(&mut db, "EXPLAIN SELECT name FROM emp WHERE salary >= 400 ORDER BY salary"),
        vec![
            "Project [NAME]",
            "  Project [EMP.NAME, EMP.SALARY]",
            "    IndexScan EMP USING EMP_SALARY (EMP.SALARY >= 400)",
        ]
    );
    assert_eq!(
        lines(&mut db, "SELECT name FROM emp WHERE salary >= 400 ORDER BY salary"),
        vec!["bob", "carol", "alice"]
    );
}
```
