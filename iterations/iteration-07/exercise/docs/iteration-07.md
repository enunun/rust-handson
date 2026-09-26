# Iteration 7：WHERE，列の選択，別名

このIterationでは，`SELECT`で列と式を選んで別名を付け，`WHERE`で行を絞り込む．
構文木の列の名前を行の中の番号に解決する段階(名前解決)を作り，評価はその結果を使う．
Rustでは，イテレーターとクロージャを使って，値の並びを組み立てる．

## 7-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 125 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 7-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-07.md)：イテレーター，クロージャ，`map`，`filter`，`position`，`collect`と`Result`，`ok_or_else`
- [データベースのノート](../../../../docs/db/iteration-07.md)：`SELECT`の処理の順序，名前解決，`WHERE`と3値論理，列名

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．`for`文を使わずに書く．

1. `&[i64]`の奇数だけを2乗した`Vec<i64>`を返す関数`odd_squares`を，`filter`と`map`と`collect`で書く．
2. `vec!["alice", "bob", "carol"]`の中の`"bob"`の添字を，`position`で求める．
3. `&[i64]`のうち，引数`threshold`より大きい要素の数を返す関数を書く．クロージャが`threshold`を捕捉することを確かめる．
4. `&[&str]`の各要素の文字の数を`Result<Vec<usize>, String>`で返す関数を書く．空の文字列があれば`Err`にする．

## 7-3 テストリスト

### 要件

- `SELECT a, b + 1 AS c FROM t WHERE a > 1`のように，列と式を選び，別名を付ける．`AS`は省略できる．`*`と式を混ぜてもよい．
- 結果の列名は，別名があれば別名，列そのものなら列名，それ以外の式は`?column?`とする．
- `WHERE`の条件が`TRUE`の行だけを返す．`FALSE`と`UNKNOWN`の行は返さない．
- 存在しない列は`42703`とする．`VALUES`の式は列を参照できない．
- `WHERE`の条件が真偽値でなければ`42804`とする．条件の型は，各行を評価するときに調べる．

### 使用例

```console
ferrodb> SELECT name, id * 10 AS score FROM users WHERE id >= 2;
 NAME | SCORE
------+-------
 bob  |    20
(1 row)
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `sql::token`，`sql::lexer` | キーワード`WHERE`，`AS` |
| `sql::ast` | `Expr::Column(String)`，`enum SelectItem { Wildcard, Expr { expr, alias } }`．`Select`を`items`，`from`，`filter: Option<Expr>`にする |
| `plan::binder` | `enum BoundExpr { Constant(Value), Column(usize), Unary, Binary, IsNull }`，`enum BindError`，`pub fn bind(expr: &Expr, columns: &[Column]) -> Result<BoundExpr, BindError>` |
| `exec::eval` | `pub fn eval(expr: &BoundExpr, row: &[Value])`，`pub fn eval_condition(expr: &BoundExpr, row: &[Value], clause: &'static str) -> Result<bool, EvalError>`，`EvalError::ArgumentNotBoolean` |

### 書くときに考えること

- 名前解決と評価を分けて単体テストで確かめる．名前解決のテストは`BoundExpr`を，評価のテストは値を比べる．
- `WHERE`と3値論理の境界を考える．条件が`NULL`になる行は，条件を否定しても残らないことを確かめる入力を用意する．
- 結合テストの新しいファイル`tests/select.rs`では，複数の行を持つ表を用意する補助関数を作ると，各テストが短くなる．
- 評価の対象が`Expr`から`BoundExpr`に変わると，評価の単体テストはどう変わるか．

## 7-4 設計ドキュメント

- `c4-component.md`：`plan::binder`を加える．`exec::eval`は，何を評価するようになるか．
- `code-types.md`：`BoundExpr`，`SelectItem`，`BindError`を加え，`Select`を更新する．
- `code-sequence.md`：`SELECT`の流れを描く．名前解決，行ごとの条件の評価，選択項目の評価の順序を示す．

## 7-5 テスト駆動の実装

### 実装のヒント

- 列の名前は，式の最小の部品(`operand`)に加える．識別子を読んで`Expr::Column`にする．
- 選択項目は，`*`か，式と省略できる別名である．`opt(preceded(opt(AS), identifier))`で別名を読める．
- `value`は値を複製するので`Clone`が要る．`Clone`を導出していない型を返すときは，クロージャ`.map(|_| ...)`で値を作る．
- 名前解決は，`Expr`を再帰的にたどって`BoundExpr`を作る．列の名前は，`columns.iter().position(...)`で番号にする．
- 評価は，`BoundExpr`と行を受け取るようにする．`VALUES`と`INSERT`の式は，空の列の並びで名前解決し，空の行で評価する．
- `SELECT`では，選択項目と条件を先に名前解決してから，行ごとに評価する．`*`は，すべての列の`BoundExpr::Column`に展開する．
- 選択項目ごとの値は，`map`と`collect`で`Result<Vec<Value>, _>`にまとめる．
- 既存の`for`で`Vec`を組み立てているところ(`CREATE TABLE`の列，`INSERT`の列の番号，`VALUES`の結果)も，イテレーターで書き直す(Refactor)．

### ツールの操作

- `cargo clippy`が入れ子の`if`をまとめるよう求めたら，条件を1つの関数に分けることも考える．

## 7-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. 名前解決の結果を別の型`BoundExpr`にした．`Expr`に列の番号を書き込む列挙子を加え，同じ型のまま名前解決する設計と比べる．評価の`match`はどうなるか．
3. `WHERE`の条件の型を，名前解決のときに調べる設計と比べる．行のない表に`SELECT * FROM t WHERE 1`を実行すると，それぞれどうなるか．
4. `Database::select`で，行を絞り込むところは`for`，選択項目を評価するところは`map`と`collect`で書いた．それぞれを逆の書き方にするとどうなるか．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 7-7 発展課題

標準SQLの`式 IN (式, ...)`に対応する．`a IN (1, 2)`は`a = 1 OR a = 2`と同じ意味である．
3値論理では，`age IN (30, NULL)`は，`age`が30なら真，それ以外は`UNKNOWN`になる．
`IN`の優先順位は，比較と連結の間とする．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
