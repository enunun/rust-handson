# Iteration 8：更新，削除，制約

このIterationでは，`UPDATE`と`DELETE`で表の行を書き換え，消し，`DROP TABLE`で表を消す．
列制約`NOT NULL`，`PRIMARY KEY`，`UNIQUE`を検査し，1つの文は全体として成功するか，何も変えないかのどちらかにする．
Rustでは，`Vec`の要素をその場で書き換える方法と，そのときに起きる借用の衝突の避け方を学ぶ．

## 8-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 136 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo run`でREPLを起動し，`UPDATE users SET name = 'x';`がまだ構文エラーになることを確かめる．

## 8-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-08.md)：`iter_mut`による書き換え，`retain`，借用の衝突，計算と変更の分離，`if let`，`HashSet`と`Hash`の導出
- [データベースのノート](../../../../docs/db/iteration-08.md)：`UPDATE`と`DELETE`，整合性制約，制約の名前，文単位の原子性，制約を検査する時点

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `&mut [i32]`の負の要素を`0`に書き換える関数を，`iter_mut`で書く．
2. `Vec<&str>`から空の文字列を`retain`で消す．
3. `&[&str]`に同じ文字列が2度以上現れるかを返す関数`has_duplicate`を，`HashSet`の`insert`の戻り値を使って書く．
4. `&mut [i32]`の各要素を「最小の要素との差」に書き換える関数を書く．まず，最小の要素を`for`の中で求めて，コンパイラーのエラーを読む．次に，計算と変更を分けて直す．`min`は，イテレーターの最小の要素を`Option`で返す．

## 8-3 テストリスト

### 要件

- `UPDATE t SET a = a + 1 WHERE ...`と`DELETE FROM t WHERE ...`を扱う．結果は`UPDATE n`，`DELETE n`とする．
- `DROP TABLE t`で表を消す．
- 列制約`NOT NULL`，`PRIMARY KEY`，`UNIQUE`を扱う．違反は`23502`，`23505`とする．
- `PRIMARY KEY`は`NOT NULL`と`UNIQUE`を合わせたものとする．表に2つ書いたら`42P16`とする．
- `UNIQUE`の列には`NULL`をいくつでも入れられる．
- 1つの文の途中で制約に違反したら，その文による変更をすべて取り消す．制約は文の終わりに検査する．
- `SET`の式は書き換える前の行で評価する．同じ列に2度代入したら`42601`とする．
- ない表の`DROP TABLE`は`42P01`とする．

エラーのメッセージは，PostgreSQLに合わせて次のようにする．

| SQLSTATE | メッセージ |
| --- | --- |
| `23502` | `null value in column "NAME" of relation "USERS" violates not-null constraint` |
| `23505` | `duplicate key value violates unique constraint "USERS_PKEY"` |
| `42P16` | `multiple primary keys for table "T" are not allowed` |
| `42601` | `multiple assignments to same column "AGE"` |
| `42P01` | `table "T" does not exist`(`DROP TABLE`のとき) |

制約の名前は，`PRIMARY KEY`なら`表_PKEY`，`UNIQUE`なら`表_列_KEY`とする．

### 使用例

```console
ferrodb> CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL);
CREATE TABLE
ferrodb> INSERT INTO users VALUES (1, 'alice'), (1, 'bob');
ERROR:  duplicate key value violates unique constraint "USERS_PKEY"
ferrodb> UPDATE users SET name = 'carol' WHERE id = 1;
UPDATE 0
```

`INSERT`が2行とも取り消されたので，`UPDATE`の対象になる行はない．

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `sql::token`，`sql::lexer` | キーワード`UPDATE`，`SET`，`DELETE`，`DROP`，`PRIMARY`，`KEY`，`UNIQUE` |
| `sql::ast` | `enum ColumnConstraint { NotNull, PrimaryKey, Unique }`，`ColumnDef::constraints: Vec<ColumnConstraint>`，`struct Update { table, assignments: Vec<Assignment>, filter }`，`struct Assignment { column, value: Expr }`，`struct Delete { table, filter }`，`struct DropTable { name }`と，それぞれの`Statement`の列挙子 |
| `value` | `pub type Row = Vec<Value>;` |
| `catalog` | `Column::nullable: bool`，`struct UniqueConstraint { name: String, column: usize }`，`TableSchema::unique_constraints`，`Catalog::drop_table(&mut self, name: &str) -> Result<(), SchemaError>` |
| `exec::dml` | `enum ConstraintError`，`pub fn insert(schema: &TableSchema, rows: &mut Vec<Row>, new_rows: Vec<Row>) -> Result<usize, Error>`，`pub fn update(schema: &TableSchema, rows: &mut Vec<Row>, assignments: &[(usize, BoundExpr)], filter: &Option<BoundExpr>) -> Result<usize, Error>`，`pub fn delete(rows: &mut Vec<Row>, filter: &Option<BoundExpr>) -> Result<usize, Error>` |
| `database` | `StatementResult::Update { count }`，`Delete { count }`，`DropTable` |
| `error` | `SqlState`の`NotNullViolation`，`UniqueViolation`，`InvalidTableDefinition`と，`ConstraintError`からの変換 |

`insert`，`update`，`delete`は，変えた行の数を返す．

### 書くときに考えること

- `exec::dml`の単体テストでは，`TableSchema`と行の並びを直接作って関数に渡せる．SQLの文を通さずに，制約の境界(`NULL`が2つある`UNIQUE`の列など)を確かめられる．
- 「何も変えない」ことを確かめるには，エラーのあとに表の行を読み，文の前と同じであることを比べる．
- 制約に違反する例だけでなく，評価のエラー(0による除算)が途中の行で起きる例も考える．
- 文の終わりに制約を検査することは，どんな`UPDATE`で確かめられるか．
- 既存のテストのうち，`ColumnDef`や`Column`を作っているものは，どう変わるか．
- 結合テストは，`UPDATE`と`DELETE`の`tests/dml.rs`と，制約の`tests/constraints.rs`に分け，`DROP TABLE`は`tests/tables.rs`に加えるとよい．

## 8-4 設計ドキュメント

- `c4-component.md`：`exec::dml`を加える．`exec::dml`は，どのモジュールの何を使うか．`database`から`exec::dml`へは何を呼ぶか．
- `code-types.md`：制約の型(`ColumnConstraint`，`UniqueConstraint`，`ConstraintError`)と，`Update`，`Assignment`，`Delete`，`DropTable`を加える．`Statement`，`Column`，`TableSchema`，`StatementResult`なども更新する．
- `code-sequence.md`：`UPDATE`の流れを描く．新しい行をいつ計算し，制約をいつ検査し，表をいつ書き換えるかを示す．

更新したら，リポジトリのルートでMermaidの構文を検査する．

## 8-5 テスト駆動の実装

### 実装のヒント

- `NOT NULL`と`PRIMARY KEY`は2つのキーワードの組である．列の定義のあとの制約は，`repeat(0.., ...)`で0個以上読む．
- `UPDATE`の構文は，`UPDATE`のあとを`cut_err`で囲む．`SET`のあとの代入は，`separated(1.., ...)`で1つ以上読む．
- `CREATE TABLE`の列の定義から`Column`と`UniqueConstraint`を作る関数を`database`に書く．`Vec`の`contains`で，列の定義が制約を持つかを調べられる．
- 制約の検査は，変更したあとの表のすべての行を受け取る関数にする．一意性は，`HashSet`に列の値を`insert`して，`false`が返ったら違反である．`HashSet<&Value>`を使うには，`Value`に`Eq`と`Hash`を導出する．
- `update`は，計算と変更を分ける．
  1. 各行について，条件を満たせば新しい行を，満たさなければ`None`を計算する．代入の式は元の行で評価し，`Column::assign`で列の型に合わせる．
  2. 表の行を複製し，`iter_mut`と`zip`で新しい行を書き込む．
  3. 複製した行の制約を検査し，よければ表の行を複製で置き換える．
- `delete`は，先にすべての行の条件を評価して真偽値の並びを作り，エラーがなければ`retain`で消す．
- `insert`も，表の行の複製に新しい行を加えてから制約を検査する．`Database::insert`は，列の型に合わせた行を`exec::dml::insert`に渡す．
- `Database`のメソッドでは，`self.catalog`から引いた`&TableSchema`を持ったまま，`self.rows.get_mut(...)`で行を`&mut`で借用できる．別のフィールドだからである．行を返す`&mut self`のメソッドを作ると，借用が衝突する．
- `SELECT`の`matches_filter`は`UPDATE`と`DELETE`でも使う．`exec::eval`に移す(Refactor)．

### ツールの操作

- `exec::dml`の単体テストだけを実行して，Red → Greenを速く回す．
- `cargo clippy`を実行し，警告を直す．

## 8-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. 制約の検査のために，表のすべての行を複製した．複製せずに，新しい行だけを既存の行と照らし合わせる設計を考える．`INSERT`と`UPDATE`のそれぞれで，どの行どうしを比べればよいか．
3. `NOT NULL`を`Column::nullable`，一意性制約を`TableSchema::unique_constraints`で表した．`Column`に`constraints: Vec<ColumnConstraint>`をそのまま持たせる設計と比べる．「`PRIMARY KEY`の列は`NULL`を持てない」ことを，検査のコードはどう知るか．
4. 「一意性は文の終わりに検査する」ことを，`exec::dml`の単体テストと結合テストの両方で確かめた．それぞれのテストが失敗したとき，原因を探す範囲はどう違うか．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 8-7 発展課題

標準SQLの表制約`PRIMARY KEY (列)`と`UNIQUE (列)`に対応する．列の定義と同じ括弧の中に，`,`で区切って書く．

```sql
CREATE TABLE t (a INTEGER, b INTEGER, PRIMARY KEY (a), UNIQUE (b));
```

括弧の中に書ける列は1つとする．意味は，その列に列制約を書いたときと同じである．
括弧の中の列が表にない場合は，`42703`と`column "C" named in key does not exist`とする．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
