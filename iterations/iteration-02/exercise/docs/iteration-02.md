# Iteration 2：真偽値，比較，NULLと3値論理

このIterationでは，値に真偽値と`NULL`を加え，比較演算子と論理演算子を3値論理で評価する．
演算子の種類を型で分類し，扱う値の組み合わせをタプルの`match`で書く．

## 2-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
     Running unittests src/lib.rs (target/debug/deps/ferrodb-fcec4bc47a054b29)
test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/tokenize.rs (target/debug/deps/tokenize-8d7c29559361cdcd)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/values.rs (target/debug/deps/values-920a463a58df66c3)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 2-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-02.md)：`impl`ブロック，メソッドと関連関数，`Option<bool>`による3値，タプルの`match`，型による分類，`Ordering`
- [データベースのノート](../../../../docs/db/iteration-02.md)：`NULL`，3値論理，`IS NULL`，演算子の優先順位

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. 幅と高さを持つ`struct Rectangle`に，関連関数`new`，面積を返すメソッド`area`，幅と高さを何倍かにするメソッド`scale`を定義する．`scale`は`&mut self`を受け取る．
2. `Option<bool>`を2つ受け取り，3値論理の`OR`を返す関数`either`を，タプルの`match`で書く．データベースのノートの真理値表のとおりになることを確かめる．
3. `Dog`，`Cat`，`Bird`の列挙子を持つ`enum Animal`と，足の数を返す関数を書く．`Dog`と`Cat`を1つの腕にまとめる．
4. `3.cmp(&5)`と`true.cmp(&false)`の結果を確かめる．

## 2-3 テストリスト

### 要件

- リテラル`TRUE`，`FALSE`，`UNKNOWN`，`NULL`を扱う．
- 比較演算子`=`，`<>`，`<`，`<=`，`>`，`>=`と，論理演算子`AND`，`OR`，`NOT`を扱う．
- `NULL`との比較は`UNKNOWN`になる．`AND`と`OR`は3値論理の真理値表に従う．
- `IS NULL`と`IS NOT NULL`を扱う．
- 算術演算の片方が`NULL`なら結果は`NULL`になる．
- 型の合わない演算(`1 + TRUE`など)はエラーにする．

### 使用例

```rust
let result = ferrodb::execute("VALUES (1 < 2 AND NULL, NULL IS NULL, 1 + NULL)").unwrap();
assert_eq!(result.rows, vec![vec![Value::Null, Value::Boolean(true), Value::Null]]);
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `token` | 比較演算子のトークンと，キーワード`TRUE`，`FALSE`，`UNKNOWN`，`NULL`，`AND`，`OR`，`NOT`，`IS` |
| `value` | `Value::Boolean(bool)`，`Value::Null`，メソッド`is_null(&self) -> bool`，関連関数`from_truth(truth: Option<bool>) -> Value` |
| `ast` | `Expr::Boolean`，`Expr::Null`，`Expr::IsNull { operand, negated }`，`UnaryOp::Not`．`BinaryOp`を`Arithmetic(ArithmeticOp)`，`Comparison(ComparisonOp)`，`And`，`Or`に分ける |
| `eval` | `EvalError::DatatypeMismatch` |

### 書くときに考えること

- `BinaryOp`を分けると，既存の構文解析のテストの期待値が変わる．どのテストのどの部分が変わるかを，テストリストの最初の項目にする．
- 3値論理は，真理値表の組み合わせを漏れなく確かめる．`AND`と`OR`で結果が不明になる組み合わせはどれか．
- 優先順位の項目は，隣り合う強さの演算子の組ごとに1つ考える．
- 型の不一致は，算術，比較，論理演算のそれぞれで起きる．`NULL`と真偽値の算術はどうなるべきか．

## 2-4 設計ドキュメント

- `c4-component.md`：新しいモジュールはない．モジュールの間の依存が変わるかを確かめる．
- `code-types.md`：`Value`と`Expr`の新しい列挙子，演算子を分類する型．`Value`のメソッドと関連関数も書く．
- `code-sequence.md`：流れは変わらない．図の下の説明に，演算子の優先順位と3値論理の扱いを書く．

## 2-5 テスト駆動の実装

### 実装のヒント

- 字句解析では，`<>`と`<=`を`<`より先に試す．`alt`は最初に成功した選択肢を使う．
- `alt`の選択肢が多すぎてコンパイルエラーになったら，種類ごとのパーサーに分けるか，`alt`を入れ子にする．
- キーワードの判定は，`to_ascii_uppercase()`で大文字にしてから`match`で比べると，キーワードの表として読める．
- 定数(整数，`TRUE`，`NULL`など)は，`any.verify_map`に渡す1つの関数で読める．
- `IS NULL`を後置演算子(`postfix`)，`NOT`を前置演算子として読む．比較は，結合性のない中置演算子(`Infix::Neither`)として読む．
- 算術演算と比較は，`(left, right)`のタプルに対する`match`で，型の組み合わせごとに分ける．
- `AND`，`OR`，`NOT`は，両辺を`Option<bool>`に変換してから真理値表で計算する．変換できない値(整数)は型の不一致である．
- 比較は，`cmp`で`Ordering`を得てから，演算子ごとに判定する．

### ツールの操作

- 字句解析のテストだけ，構文解析のテストだけ，というようにモジュールの名前で絞り込んで実行する．
- `cargo clippy`が`Option::map`で書けると指摘したら，クロージャ(Iteration 7で扱う)を使わずに，真理値表の形で書いてもよい．

## 2-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．3値論理の組み合わせと優先順位を，どこまで挙げられたか．
2. `NULL`を`Value::Null`という列挙子で表した．代わりに，値を`Option<Value>`で包み，`None`を`NULL`とする設計と比べる．
   - `NULL`でない値だけを受け取る関数は，それぞれの設計でどう書けるか．
   - 3値論理の`Option<bool>`と，`Option<Value>`はどう対応するか．
   - Iteration 5で表の列に型が付くと，「`INTEGER`型の`NULL`」を表す必要はあるか．
3. `BinaryOp`を分けずに12個の列挙子を持つ`enum`にした場合，算術演算を評価する関数の`match`はどうなるか．
4. `Value::is_null`はメソッド，`Value::from_truth`は関連関数にした．それぞれを，もう一方の形にしなかった理由は何か．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 2-7 発展課題

標準SQLの`IS TRUE`と`IS FALSE`(と，それぞれの`IS NOT`)に対応する．
`NULL IS TRUE`は偽，`1 < 2 IS TRUE`は真になる．結果は不明にならない．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
