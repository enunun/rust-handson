# Iteration 17：インデックスの利用

Iteration 16で作ったB+木を，表のインデックスとして使う．
`CREATE INDEX`でインデックスを作り，`WHERE`の条件に合う行をインデックスで読む．`PRIMARY KEY`と`UNIQUE`の列には自動でインデックスを作り，一意性制約をインデックスで検査する．
Rustでは，型の違う値を`enum`でまとめる方法とトレイトオブジェクトとの違い，`Bound`の操作，構造体のフィールドを別々に借りる方法を学ぶ．

## 17-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 275 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.84s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.14s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

REPLで`EMP`の表を作り，`EXPLAIN SELECT name FROM emp WHERE salary >= 450;`の実行計画を見ておく．

## 17-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-17.md)：`enum`で型の違う値をまとめる，組の`match`，トレイトオブジェクトとの比較，`Bound`の`as_ref`と`map`，フィールドを別々に借りる
- [データベースのノート](../../../../docs/db/iteration-17.md)：インデックスの作成と更新，一意性制約とインデックス，インデックススキャン，キーの範囲と残りの条件，全件走査との選択

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `i64`か`String`の値を持つ`enum Key`と，`Vec<i64>`か`Vec<String>`を持つ`enum Keys`を作る．`Keys::push(&mut self, key: Key) -> Result<(), String>`を，`match (self, key)`で書く．型が合わなければ`Err`を返す．
2. 2つの上限`Bound<i32>`のうち狭い方を返す`fn tighter_upper(a: Bound<i32>, b: Bound<i32>) -> Bound<i32>`を書く．値が同じなら`Excluded`の方が狭い．
3. `names: Vec<String>`と`lengths: Vec<usize>`を持つ構造体に，`lengths`を`&mut`で，`names`を`&`で同時に借りて，名前の長さを書き込むメソッドを書く．

## 17-3 テストリスト

### 要件

- `CREATE INDEX name ON t (col)`と`DROP INDEX name`で，1列のインデックスを作る．既存の行もインデックスに入れる．
- `PRIMARY KEY`と`UNIQUE`の列には，自動でインデックスを作る．一意性の検査はインデックスで行う．
- `WHERE col = 定数`と，`<`，`<=`，`>`，`>=`を`AND`でつないだ条件にインデックスが使えるなら，インデックススキャンを選ぶ．
- `INSERT`，`UPDATE`，`DELETE`でインデックスも更新する．
- 一意性制約のインデックスは，制約と同じ名前(`EMP_PKEY`など)で作る．表とインデックスは同じ名前を使えない(`42P07`)．
- ないインデックスの`DROP INDEX`は`42704`とし，メッセージを`index "X" does not exist`とする．一意性制約のインデックスの`DROP INDEX`は`2BP01`とし，メッセージを`cannot drop index EMP_PKEY because constraint EMP_PKEY on table EMP requires it`とする．
- `NULL`はインデックスに入れない．インデックスを使うのは，1つの表を読む`SELECT`とする．
- キーにする値が2000バイトを超えたら`54000`とし，メッセージを`index key size 2001 exceeds maximum 2000`の形にする．
- インデックスは，名前のUTF-8のバイトの16進数に`.index`を付けたファイルに置く．

### 使用例

```console
ferrodb> CREATE TABLE emp (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL, dept VARCHAR(10), salary INTEGER);
CREATE TABLE
ferrodb> INSERT INTO emp VALUES (1, 'alice', 'dev', 500), (2, 'bob', 'dev', 400), (3, 'carol', 'ops', 450), (4, 'dave', NULL, NULL);
INSERT 0 4
ferrodb> EXPLAIN SELECT name FROM emp WHERE salary >= 450;
          QUERY PLAN
------------------------------
 Project [EMP.NAME]
   Filter (EMP.SALARY >= 450)
     SeqScan EMP
(3 rows)

ferrodb> CREATE INDEX emp_salary ON emp (salary);
CREATE INDEX
ferrodb> EXPLAIN SELECT name FROM emp WHERE salary >= 450;
                      QUERY PLAN
------------------------------------------------------
 Project [EMP.NAME]
   IndexScan EMP USING EMP_SALARY (EMP.SALARY >= 450)
(2 rows)

ferrodb> SELECT name FROM emp WHERE salary >= 450;
 NAME
-------
 carol
 alice
(2 rows)

ferrodb> EXPLAIN SELECT name FROM emp WHERE id = 3;
                 QUERY PLAN
---------------------------------------------
 Project [EMP.NAME]
   IndexScan EMP USING EMP_PKEY (EMP.ID = 3)
(2 rows)

ferrodb> DROP INDEX emp_pkey;
ERROR:  cannot drop index EMP_PKEY because constraint EMP_PKEY on table EMP requires it
```

`EXPLAIN`では，`IndexScan 表 USING インデックス (範囲を決めた条件)`と表示する．範囲を決めた条件のほかに条件があれば，`IndexScan`の上の`Filter`に置く．

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `sql::parser` | `Statement::CreateIndex(CreateIndex { name, table, column })`と`Statement::DropIndex(DropIndex { name })`．キーワード`INDEX` |
| `catalog` | `pub struct IndexDef { name, table, column: usize }`，`create_index`，`drop_index`，`index`，`indexes_of(table)`．カタログのファイルにインデックスの定義を加える |
| `index` | `pub enum AnyIndex<'a> { Integer(BTree<'a, i32>), BigInt(..), Boolean(..), Varchar(..) }`と`create`，`open`，`insert`，`delete`，`range(lower: Bound<&Value>, upper: Bound<&Value>)`，`lookup`．`pub struct ColumnIndex<'a> { column, index }` |
| `index::btree` | Iteration 16の発展課題の`BTree::open` |
| `storage::heap` | `HeapFile::get(id: RowId) -> Result<Option<Vec<u8>>, HeapError>` |
| `plan::planner` | `PlanNode::IndexScan { table, alias, columns, index, lower, upper, conditions }`，`plan(select, catalog)` |
| `exec::index_scan` | `IndexScan` |
| `exec::dml` | `insert`，`update`，`delete`が`&mut [ColumnIndex<'_>]`を受け取る |
| `database` | インデックスごとのバッファプール，`CREATE INDEX`，`DROP INDEX`，`StatementResult::CreateIndex`，`StatementResult::DropIndex` |
| `error` | `SqlState::UndefinedObject`(`42704`)，`SqlState::DependentObjectsStillExist`(`2BP01`)と，`BTreeError`からの変換 |

### 書くときに考えること

- インデックスを使う実行計画は，プランナーの単体テストで確かめる．テストのカタログにインデックスを加えると，引き継いだプランナーのテストのどれが変わるか．
- 範囲を決める条件と，`Filter`に残す条件の組み合わせを考える．インデックスを使えない条件の例も挙げる．
- 一意性制約をインデックスで検査すると，「表のすべての行を調べる」関数の単体テストはどうなるか．
- `UPDATE t SET id = id + 1`が成功することを，インデックスで検査しても保つ．
- 行を変えたあとで，インデックスで引いた行が正しいかをどう確かめるか．
- インデックスを作るときに，キーにできない値の行があるとする．そのときは，カタログとファイルのどちらにもインデックスが残らないことを確かめる．
- 開き直したデータベースでもインデックスが使えることを，結合テストで確かめる．

## 17-4 設計ドキュメント

- `c4-container.md`：データディレクトリにインデックスのファイルを加える．
- `c4-component.md`：`index`モジュール(`AnyIndex`)と`exec::index_scan`を加える．`database`，`plan::planner`，`exec::build`，`exec::dml`は，何に新しく依存するか．
- `code-types.md`：`AnyIndex`，`ColumnIndex`，`IndexDef`，`IndexScan`，`PlanNode::IndexScan`，文の型を加え，`Database`と`Catalog`を更新する．
- `code-sequence.md`：プランナーがインデックスを選び，`IndexScan`で行を読む流れを加える．`UPDATE`の図の制約の検査も見直す．
- `layout.md`：カタログのファイルに，インデックスの定義を加える．

更新したら，リポジトリのルートでMermaidの構文を検査し，照合スクリプトも実行する．

## 17-5 テスト駆動の実装

### 実装のヒント

- 文の`alt`に`CREATE INDEX`と`DROP INDEX`を加えると，選択肢が10個になってコンパイルエラーになる．`CREATE`で始まる文と`DROP`で始まる文を，それぞれ`alt`にまとめる．
- `Catalog::create_table`で，一意性制約ごとのインデックスの定義も加えると，表を作る側は制約を気にしなくてよい．
- `AnyIndex`のメソッドは，`match (self, value)`で列挙子と`Value`の組み合わせを選ぶ．範囲は`Bound::map`で`Value`をキーの型にする．
- `exec::dml`は，行を変える前に，`NOT NULL`と一意性を「変える行」だけで検査する．一意性は，値をインデックスで引き，見つかった位置が書き換える行でなければ違反とする．
- `HeapFile::update`は行を別のページに移すことがある．インデックスの項目は，古い値と古い位置で消し，新しい値と新しい位置で加える．
- プランナーは，`WHERE`の条件を`AND`で分け，`列 比較 定数`の形の条件を探す．定数は`Column::assign`で列の型にしてからキーの範囲にする．
- `Database`は，インデックスの名前ごとの`BufferPool<Box<dyn DiskManager>>`を持ち，文を実行するたびに`AnyIndex::open`で開く．`BufferPool`に`Debug`がないので，`Database`の`#[derive(Debug)]`のために実装する．
- `self.tables.get_mut`で表を借りたまま，`&self`のメソッドでインデックスを開こうとすると借用のエラーになる．必要なフィールドだけを受け取る関数にする．

## 17-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. `AnyIndex`を`enum`にした．`trait Index`を定義し，`Box<dyn Index>`で持つ設計と比べる．キーの型を加えるとき，操作を加えるときに，それぞれ何を直すか．
3. 一意性制約をインデックスで検査するようにした．表の行が100万行のとき，`INSERT`1行の検査で読むページの数は，Iteration 8の方法と比べてどう変わるか．
4. `WHERE salary >= 0`のように，ほとんどの行が条件に合う場合も，模範解答はインデックスを使う．全件走査より遅くなりうる理由を考える．
5. `NULL`をインデックスに入れない設計では，`WHERE salary IS NULL`にインデックスを使えない．`NULL`も入れる設計にすると，キーの符号化と一意性の検査はどう変わるか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 17-7 発展課題

`IndexScan`はキーの順に行を返す．`ORDER BY`がインデックスの列の昇順だけなら，`Sort`は要らない．
プランナーで，`Project`の下が`IndexScan`(か，その上の`Filter`)で，並べ替えのキーがインデックスの列の昇順1つだけなら，`Sort`を置かないようにする．

```console
ferrodb> EXPLAIN SELECT name FROM emp WHERE salary >= 400 ORDER BY salary;
```

は，`Sort`を含まず，次の順に演算子が重なる．

```text
Project [NAME]
  Project [EMP.NAME, EMP.SALARY]
    IndexScan EMP USING EMP_SALARY (EMP.SALARY >= 400)
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
