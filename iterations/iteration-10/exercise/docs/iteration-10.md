# Iteration 10：実行計画とEXPLAIN

このIterationでは，Iteration 7〜9で`Database::select`の1つの関数に書いた`SELECT`の処理を，演算子ごとの`Executor`に分ける．
名前を解決した`SELECT`を実行計画(演算子の木)にしてから実行し，`EXPLAIN`で実行計画を表示する．
Rustでは，トレイトを自分で定義し，違う型の演算子を`Box<dyn Executor>`で同じように扱う．

## 10-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 165 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

このIterationは，`SELECT`の結果を変えない．引き継いだ`select.rs`と`order_by.rs`の結合テストは，実装を置き換えている間も通り続けるはずである．

## 10-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-10.md)：トレイトの定義，トレイトオブジェクト，静的ディスパッチと動的ディスパッチ，`while let`，`let ... else`
- [データベースのノート](../../../../docs/db/iteration-10.md)：問い合わせの処理の段階，論理計画と物理計画，Volcanoモデル，`EXPLAIN`

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. 面積を返すメソッド`fn area(&self) -> u32`を持つトレイト`Shape`を定義し，長方形`Rect`と正方形`Square`に実装する．`&[Box<dyn Shape>]`の面積の合計を返す関数を書き，長方形と正方形を混ぜた`Vec`で確かめる．
2. 課題1の関数を，ジェネリクス`fn total_area_of<S: Shape>(shapes: &[S]) -> u32`でも書く．長方形と正方形を混ぜた配列を渡し，コンパイラーのエラーを読む．
3. `fn next(&mut self) -> Option<i32>`を持つトレイト`Source`を定義する．`current`から`end`の手前までの整数を返す`Range`と，子の`Source`の先頭の`count`個を飛ばす`Skip { input: Box<dyn Source>, count: usize }`に実装する．`while let`で`Skip`の値をすべて集めて確かめる．

## 10-3 テストリスト

### 要件

- `SELECT`を，実行計画(演算子の木)に変換してから実行する．結果は変えない．
- `EXPLAIN SELECT ...`で実行計画を表示する．演算子を1行に1つ，子を親より2文字深く字下げして書く．`EXPLAIN`は演算子を動かさない．
- 結果の列にない式で並べ替えるときは，その式を`Project`の隠れた列として計算し，最上段の`Project`で取り除く．

`EXPLAIN`の結果は，列名が`QUERY PLAN`の1列の表である．各演算子は次のように表す．

| 演算子 | 表示 | 置く条件 |
| --- | --- | --- |
| `SeqScan` | `SeqScan 表` | 常に置く |
| `Filter` | `Filter 条件` | `WHERE`があるとき |
| `Project` | `Project [式, ...]` | 常に置く |
| `Distinct` | `Distinct` | `DISTINCT`があるとき |
| `Sort` | `Sort [キー, ...]` | `ORDER BY`があるとき |
| `Limit` | `Limit OFFSET n FETCH FIRST m` | `OFFSET`か`FETCH FIRST`があるとき．書かれた方だけを表示する |

- 式は，列を子の演算子の列名で書き，2項演算，単項演算，`IS NULL`を括弧で囲む(`(ID > 1)`，`((ID * 10) > 1)`，`(-ID)`，`(NOT X)`，`(NAME IS NULL)`)．文字列の定数は`'it''s'`のように引用符で囲む．
- 並べ替えのキーは，降順なら`DESC`を付ける．`NULL`の位置が既定と違えば，`NULLS FIRST`か`NULLS LAST`を付ける．

### 使用例

```console
ferrodb> EXPLAIN SELECT name FROM users WHERE id > 1 ORDER BY name;
     QUERY PLAN
---------------------
 Sort [NAME]
   Project [NAME]
     Filter (ID > 1)
       SeqScan USERS
(4 rows)
```

結果の列にない`id`で並べ替えると，隠れた列を取り除く`Project`が最上段に来る．

```console
ferrodb> EXPLAIN SELECT name FROM users ORDER BY id;
       QUERY PLAN
------------------------
 Project [NAME]
   Sort [ID]
     Project [NAME, ID]
       SeqScan USERS
(4 rows)
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `sql::token`，`sql::lexer`，`sql::ast`，`sql::parser` | キーワード`EXPLAIN`，`Statement::Explain(Select)` |
| `plan::binder` | `struct BoundSelect`，`struct BoundOrderBy`，`enum SortSource`，`pub fn bind_select(select: &Select, schema: &TableSchema) -> Result<BoundSelect, BindError>`，`BoundExpr::display(&self, columns: &[String]) -> String`．`SortOrder`を`exec::sort`から移す |
| `plan::planner` | `enum PlanNode`，`struct SortKey { expr: BoundExpr, order: SortOrder }`，`PlanNode::columns(&self) -> &[String]`，`pub fn plan(select: &BoundSelect) -> PlanNode` |
| `plan::explain` | `pub fn explain(plan: &PlanNode) -> Vec<String>` |
| `exec` | `pub trait Executor { fn next(&mut self) -> Result<Option<Row>, Error>; }` |
| `exec::scan`，`exec::filter`，`exec::project`，`exec::distinct`，`exec::sort`，`exec::limit` | 演算子`SeqScan`，`Filter`，`Project`，`Distinct`，`Sort`，`Limit`と，それぞれの`new` |
| `exec::build` | `pub fn build(plan: &PlanNode, tables: &HashMap<String, Vec<Row>>) -> Box<dyn Executor>`，`pub fn collect_rows(executor: &mut dyn Executor) -> Result<Vec<Row>, Error>` |

### 書くときに考えること

- 演算子の単体テストでは，子に`SeqScan`を置けば，好きな行を流し込める．`collect_rows`で結果をまとめて比べる．
- 実行計画の単体テストでは，演算子の種類と重なる順序を確かめる．入れ子の列挙型から子を取り出すには`let ... else`が使える．
- `EXPLAIN`の表示は，式の表し方(括弧，列名，定数)と，演算子の字下げを分けて確かめる．
- どの既存テストが，リファクタリングの安全網になるか．

## 10-4 設計ドキュメント

- `c4-component.md`：`plan::planner`，`plan::explain`，`exec::build`と演算子のモジュールを加える．`exec`が子モジュールを宣言するだけのモジュールでなくなることに注意する．`database`が直接使うモジュールはどう変わるか．
- `code-types.md`：`BoundSelect`，`PlanNode`，`SortKey`，`Executor`と演算子の型を加える．トレイトは`<<trait>>`で，実装は`<|..`で描く．構文木の`Limit`と演算子の`Limit`をどう描き分けるかも考える．
- `code-sequence.md`：`SELECT`の流れを，各演算子が`next`で1行ずつ親の演算子へ渡す流れにする．

更新したら，リポジトリのルートでMermaidの構文を検査する．

## 10-5 テスト駆動の実装

### 実装のヒント

- 最初に`bind_select`を作り，`database`にある選択項目と`ORDER BY`の名前解決を`plan::binder`に移す．移したあとも既存のテストが通ることを確かめる．
- 次に，演算子を1つずつ作る．どの演算子も，子の演算子を`Box<dyn Executor>`で持ち，`next`で子の`next`を呼ぶ．
- `Filter`と`Distinct`は，返せる行が見つかるまで子の`next`を呼ぶ．`while let Some(row) = self.input.next()? { ... }`の形になる．
- `Sort`は，最初の`next`で子の行をすべて読んで並べ替え，残りを`Option<std::vec::IntoIter<Row>>`に持つ．
- `Limit`は，`offset`行を飛ばしてから，残りの行の数を数えながら返す．
- `SeqScan`は，表の行の複製を持つ．Iteration 13からは，表の行をページから読む．
- `plan`は，演算子を下から`SeqScan`，`Filter`，`Project`，`Distinct`，`Sort`，`Limit`の順に重ねる．結果の列にないキーは`Project`の隠れた列にし，`Sort`のキーはその列を指す`BoundExpr::Column`にする．
- `PlanNode::columns`は，各演算子が返す行の列名を返す．`Project`は自分の列名を，それ以外は子の列名を返す．`EXPLAIN`で式を表示するときと，結果の列名に使う．
- `Database::select`は，`bind_select`，`plan`，`build`，`collect_rows`を順に呼ぶだけになる．`Box<dyn Executor>`から`&mut dyn Executor`は`as_mut()`で得る．

### ツールの操作

- 演算子を置き換える途中で，既存の結合テスト`select`と`order_by`だけを実行し，結果が変わっていないことを確かめる．

## 10-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. 演算子の子を`Box<dyn Executor>`で持った．`Filter<E: Executor>`のようにジェネリクスで持つ設計と比べる．実行計画の形がSQLの文によって変わるとき，`build`の戻り値の型はどうなるか．
3. `PlanNode`(列挙型)と`Executor`(トレイト)の2つの木を作った．実行計画を直接`Executor`の木として作り，`EXPLAIN`もそこから表示する設計と比べる．新しい演算子を加えるとき，それぞれ何か所を変えることになるか．
4. `Sort`はブロッキング演算子である．`SELECT * FROM t ORDER BY a FETCH FIRST 1 ROWS ONLY`で，`Sort`は子から何行を読むか．`ORDER BY`がないときの`Limit`と比べる．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 10-7 発展課題

`Limit`の下に`Sort`があり，`FETCH FIRST`で行の数が決まっているとき，並べ替えの結果は先頭の`OFFSET + FETCH FIRST`行しか使われない．
この場合に`Sort`の代わりに置く演算子`TopN`を作る．`TopN`は子の行を読みながら，並べ替えて先頭になる行だけを手元に残す．

```console
ferrodb> EXPLAIN SELECT name FROM users ORDER BY name DESC OFFSET 1 ROWS FETCH FIRST 1 ROWS ONLY;
```

を実行すると，実行計画の`Sort [NAME DESC]`が`TopN [NAME DESC] 2`(残す行の数が2)になる．結果は変えない．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
